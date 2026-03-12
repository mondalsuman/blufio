# Phase 57: Prompt Injection Defense - Research

**Researched:** 2026-03-12
**Domain:** Prompt injection defense, cryptographic boundary enforcement, output screening, human-in-the-loop confirmation
**Confidence:** HIGH

## Summary

Phase 57 implements a 5-layer prompt injection defense system for the Blufio agent loop. The architecture is well-specified in CONTEXT.md with locked decisions covering L1 (pattern classifier), L3 (HMAC boundary tokens), L4 (output screening), and L5 (human-in-the-loop). The existing codebase provides strong foundations: `blufio-security::pii` demonstrates the exact RegexSet pattern needed for L1, `blufio-security::redact` provides the credential pattern registry to extend for L4, `ring 0.17` already includes both HKDF and HMAC-SHA256 in the workspace, and the EventBus has a proven 14-variant pattern to extend with SecurityEvent.

The phase creates a new `blufio-injection` crate with clear separation from `blufio-security` (different concerns: PII/redaction vs. injection defense). Integration points are well-identified: `SessionActor::handle_message()` for L1, `ContextEngine::assemble()` for L3, `SessionActor::execute_tools()` for L4/L5. The pipeline is synchronous pre-LLM for L1/L3 and post-LLM for L4/L5, which aligns with the existing async architecture.

**Primary recommendation:** Build L1 pattern classifier first (mirrors proven PII RegexSet architecture), then L3 HMAC boundaries (independent crypto module), then L4 output screening (extends existing redact infrastructure), then L5 HITL (new interaction flow), and finally MCP/WASM integration (INJC-06) and CLI tooling.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **New crate:** `blufio-injection` crate (separate from `blufio-security`)
- **L1 Pattern Categories:** Role hijacking ("ignore previous", "you are now"), instruction override ("system:", "[INST]"), data exfiltration ("send to", "forward to") -- case-insensitive
- **L1 Scoring:** weighted sum of pattern severity (0.1-0.5) + match count + position in message, producing 0.0-1.0
- **L1 Pipeline Position:** synchronous, pre-LLM, <1ms via pre-compiled RegexSet
- **L1 Blocking Thresholds:** >0.95 for user input, >0.98 for MCP/WASM output
- **L1 Mode:** log-not-block by default, blocking only at threshold
- **L1 Blocked Response:** generic "I can't process this message." -- never reveal detection reason
- **L1 Extensibility:** hardcoded defaults + TOML custom patterns via `[injection_defense.input_detection.custom_patterns]`
- **L1 Reporting:** operator-only by default, users see nothing unless blocked
- **L3 HMAC Key:** per-session key derived from session ID + server secret (HKDF from vault master key with "hmac-boundary" context)
- **L3 Token Format:** version prefix (v1), transparent markers verified and stripped before LLM sees them
- **L3 Failure Action:** strip failed zone from context + log SecurityEvent
- **L3 Zone Coverage:** all three zones (static, conditional, dynamic) get HMAC boundaries
- **L3 Provenance:** each bounded zone includes source metadata (user, mcp:tool_name, skill:name, system)
- **L3 Compaction/Truncation:** re-sign after compaction or truncation
- **L3 Dev Mode:** `injection_defense.hmac_boundaries.enabled = false` in TOML
- **L4 Screens For:** credentials (known provider API key formats) + injection relay (heuristic pattern matching on LLM output)
- **L4 When:** before tool execution only; stream text to user in real-time, buffer tool call arguments for screening
- **L4 Credential Action:** redact + continue with [REDACTED]
- **L4 Relay Action:** block tool execution entirely
- **L4 Reuse:** extend existing RedactingWriter/redact infrastructure with credential patterns (shared pattern registry)
- **L4 Provider Patterns:** Anthropic (sk-ant-*), OpenAI (sk-*), AWS (AKIA*), database connection strings
- **L4 Escalation:** 3 screening failures in session = escalate to HITL for all subsequent tool calls
- **L5 Scope:** external tool execution (MCP tools, WASM skills) requires confirmation; internal tools auto-approved
- **L5 Delivery:** inline message + reply in conversation ("Approve [tool_name] with args [...]? Reply YES/NO")
- **L5 Timeout:** auto-deny after configurable 60s default
- **L5 Trust Session:** user approves once per tool type per session, subsequent auto-approved
- **L5 Safe Tools:** memory_search, session_history, cost_lookup, skill_list (configurable)
- **L5 API Bypass:** API requests trusted (programmatic trust), HITL only for interactive channels
- **L5 Multi-agent:** Ed25519-signed inter-agent messages bypass HITL
- **L5 Non-interactive:** auto-deny with log if channel can't support reply-based confirmation
- **L5 Max Pending:** 3 confirmations queued without response = pause and notify
- **Pipeline Order:** L1 -> L3 -> LLM -> L4 -> L5
- **Cross-layer Escalation:** L1 flagged context (even below blocking threshold) causes L4/L5 to apply stricter rules
- **Correlation ID:** message-level ID flows through L1->L3->L4->L5
- **Config:** `[injection_defense.*]` nested TOML sections, enabled by default in production
- **Config Defaults:** L1 log-not-block, L3 active, L4 active, L5 disabled
- **Config Override:** BLUFIO_INJECTION_ENABLED, BLUFIO_INJECTION_DRY_RUN env vars
- **Global dry_run:** injection_defense.dry_run = true simulates all layers without action
- **MCP/WASM:** separate pipeline stage from existing sanitize module, per-server trust flag
- **MCP Tool Descriptions:** scanned at discovery time for injection payloads
- **EventBus:** SecurityEvent enum with per-layer variants: InputDetection, BoundaryFailure, OutputScreening, HitlPrompt
- **Events:** go to both EventBus (real-time) AND audit trail (persistent), with full content in security events
- **Prometheus Metrics:** separate per-layer: injection_input_detections_total, hmac_validations_total{zone, result}, injection_output_screenings_total, hitl_confirmations_total, hitl_denials_total, hitl_timeouts_total
- **CLI Commands:** blufio injection test, blufio injection status, blufio injection config
- **CLI Test Output:** full scoring breakdown (patterns, scores, action, layers)
- **blufio doctor:** includes injection defense summary (active layers, pattern count, HMAC status)
- **Testing:** unit tests with attack corpus for L1 + integration tests through agent loop for L4/L5

### Claude's Discretion

- Exact regex patterns for each injection category
- HMAC token format details (byte layout, encoding)
- SecurityEvent struct field names and types
- Test attack corpus selection
- Prometheus metric label values
- HKDF derivation parameters

### Deferred Ideas (OUT OF SCOPE)

None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INJC-01 | L1 pattern classifier detects known injection signatures via regex with 0.0-1.0 confidence scoring | Mirror PII RegexSet architecture from `blufio-security::pii`. Use `regex::RegexSet` for O(1) fast path, individual `Regex` for detail extraction. OWASP cheat sheet provides canonical patterns. |
| INJC-02 | L1 operates in log-not-block mode by default, blocking only at >0.95 confidence (configurable) | Config via `[injection_defense.input_detection]` section with `mode`, `blocking_threshold` fields. SecurityEvent emitted on every detection. |
| INJC-03 | L3 HMAC-SHA256 boundary tokens cryptographically separate system/user/external content zones | `ring::hkdf::HKDF_SHA256` for key derivation, `ring::hmac::HMAC_SHA256` for signing. Per-session key via HKDF(master_key, session_id, "hmac-boundary"). Apply in `ContextEngine::assemble()`. |
| INJC-04 | L4 output validator screens LLM responses for credential leaks and injection relay before tool execution | Extend `blufio-security::redact::REDACTION_PATTERNS` with provider-specific credential regexes (AWS AKIA*, connection strings). Intercept in `SessionActor::execute_tools()`. |
| INJC-05 | L5 human-in-the-loop confirmation flow for configurable high-risk operations | New async confirmation flow in `SessionActor::execute_tools()`. Requires channel-aware delivery (inline message + response). Per-tool-type session trust cache. |
| INJC-06 | Injection defense integrates with MCP client tool output and WASM skill results | Scan tool results with L1 patterns before feeding back to LLM. Per-server trust flag in McpServerEntry. Separate from existing `sanitize` module. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `regex` | 1.x (workspace) | RegexSet for L1 pattern matching | Already used for PII detection; O(1) multi-pattern matching via RegexSet |
| `ring` | 0.17 (workspace) | HKDF-SHA256 key derivation + HMAC-SHA256 signing | Already used for AES-256-GCM in vault; provides `ring::hkdf::HKDF_SHA256` and `ring::hmac::HMAC_SHA256` |
| `base64` | 0.22 | Encoding HMAC tags for boundary tokens | Standard encoding for binary tokens in text; NOT yet in workspace |
| `hmac` | 0.12 (workspace) | Alternative HMAC if ring's API is awkward for streaming | Already in workspace; prefer `ring::hmac` since ring is already a dependency |
| `sha2` | 0.10 (workspace) | Backup for HMAC if needed | Already in workspace |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `blufio-bus` | workspace | SecurityEvent variants on EventBus | All layers emit events |
| `blufio-config` | workspace | InjectionDefenseConfig model | Config loading and validation |
| `blufio-vault` | workspace | Master key access for HKDF | L3 HMAC key derivation |
| `blufio-security` | workspace | Shared redaction patterns | L4 extends credential patterns |
| `blufio-core` | workspace | BlufioError, traits | Error handling throughout |
| `serde` | 1 (workspace) | Serialization for config, events | Standard across all crates |
| `tracing` | 0.1 (workspace) | Structured logging | All layers log decisions |
| `metrics` | 0.24 (workspace) | Prometheus counters/gauges | Per-layer metrics |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `ring::hmac` | `hmac` + `sha2` crates | Both in workspace; `ring` preferred because vault already uses ring for crypto, keeps crypto library consistent |
| `ring::hkdf` | `hkdf` crate | ring already in workspace; avoids adding another crypto dependency |
| `base64` crate | `data-encoding` | base64 is more widely used; but consider if hex encoding is simpler for boundary tokens |

**Installation (new dependency):**
```bash
# base64 needs to be added to workspace Cargo.toml
# All other dependencies already in workspace
```

**New crate setup:**
```bash
cargo new crates/blufio-injection --lib
```

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-injection/
    src/
        lib.rs              # Public API, re-exports
        config.rs           # InjectionDefenseConfig, per-layer configs
        classifier.rs       # L1: RegexSet pattern classifier with scoring
        patterns.rs         # Injection pattern definitions (single source of truth)
        boundary.rs         # L3: HMAC boundary token generation/validation
        output_screen.rs    # L4: Output screening (credentials + relay detection)
        hitl.rs             # L5: Human-in-the-loop confirmation flow
        pipeline.rs         # Pipeline coordinator, correlation ID, cross-layer escalation
        events.rs           # SecurityEvent helper constructors (mirrors pii.rs pattern)
        metrics.rs          # Prometheus metric registration and recording
    Cargo.toml
```

### Pattern 1: RegexSet Pattern Classifier (L1)
**What:** Two-phase regex detection identical to PII module architecture
**When to use:** All user and external input scanning
**Example:**
```rust
// Source: blufio-security::pii pattern, adapted for injection detection
use regex::{Regex, RegexSet};
use std::sync::LazyLock;

/// Injection pattern categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionCategory {
    RoleHijacking,
    InstructionOverride,
    DataExfiltration,
}

struct InjectionPattern {
    category: InjectionCategory,
    pattern: &'static str,
    severity: f64,  // 0.1 - 0.5
}

/// Single source of truth for all injection patterns.
/// Both INJECTION_REGEX_SET and INJECTION_INDIVIDUAL_REGEXES
/// are built from this array (same pattern as PII module).
static PATTERNS: &[InjectionPattern] = &[
    // Role hijacking
    InjectionPattern {
        category: InjectionCategory::RoleHijacking,
        pattern: r"(?i)ignore\s+(all\s+)?previous\s+instructions?",
        severity: 0.5,
    },
    InjectionPattern {
        category: InjectionCategory::RoleHijacking,
        pattern: r"(?i)you\s+are\s+now\s+",
        severity: 0.4,
    },
    // Instruction override
    InjectionPattern {
        category: InjectionCategory::InstructionOverride,
        pattern: r"(?i)^system\s*:",
        severity: 0.4,
    },
    InjectionPattern {
        category: InjectionCategory::InstructionOverride,
        pattern: r"(?i)\[INST\]",
        severity: 0.4,
    },
    // Data exfiltration
    InjectionPattern {
        category: InjectionCategory::DataExfiltration,
        pattern: r"(?i)(send|forward|email|post)\s+(to|this|all|the)",
        severity: 0.3,
    },
    // ... more patterns as needed
];

static INJECTION_REGEX_SET: LazyLock<RegexSet> = LazyLock::new(|| {
    let patterns: Vec<&str> = PATTERNS.iter().map(|p| p.pattern).collect();
    RegexSet::new(patterns).expect("injection regex patterns must compile")
});
```

### Pattern 2: HMAC Boundary Token Format (L3)
**What:** Cryptographic content zone markers for system/user/external separation
**When to use:** Context assembly before LLM receives content
**Example:**
```rust
// Source: ring 0.17 docs (docs.rs/ring/0.17.14/ring/hkdf/)
use ring::{hkdf, hmac};

/// Derive a per-session HMAC signing key using HKDF.
///
/// ikm = vault master key bytes
/// salt = session_id bytes
/// info = b"hmac-boundary"
fn derive_session_key(
    master_key: &[u8; 32],
    session_id: &str,
) -> hmac::Key {
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, session_id.as_bytes());
    let prk = salt.extract(master_key);
    let info = &["hmac-boundary".as_bytes()];
    let okm = prk.expand(info, &hmac::HMAC_SHA256)
        .expect("HKDF expand should not fail for HMAC key size");
    // ring's expand() returns an Okm that can be used to fill a key
    // For HMAC key, we extract raw bytes and construct key
    let mut key_bytes = [0u8; 32];
    okm.fill(&mut key_bytes).expect("fill 32 bytes");
    hmac::Key::new(hmac::HMAC_SHA256, &key_bytes)
}

/// Boundary token format:
/// <<BLUF-ZONE-v1:{zone}:{source}:{base64(hmac_tag)}>>
///
/// zone: "static" | "conditional" | "dynamic"
/// source: "system" | "user" | "mcp:{server_name}" | "skill:{name}"
/// hmac_tag: HMAC-SHA256(session_key, zone_content)
fn create_boundary_token(
    key: &hmac::Key,
    zone: &str,
    source: &str,
    content: &str,
) -> String {
    let tag = hmac::sign(key, content.as_bytes());
    let encoded = base64_encode(tag.as_ref());
    format!("<<BLUF-ZONE-v1:{zone}:{source}:{encoded}>>")
}
```

### Pattern 3: SecurityEvent on EventBus (follows existing pattern)
**What:** SecurityEvent as 15th BusEvent variant, with per-layer sub-variants using String fields
**When to use:** All injection defense events
**Example:**
```rust
// Source: blufio-bus::events pattern (ClassificationEvent as model)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEvent {
    /// L1: Input injection pattern detected.
    InputDetection {
        event_id: String,
        timestamp: String,
        correlation_id: String,
        source_type: String,     // "user", "mcp", "wasm"
        source_name: String,     // server/skill name or empty
        score: f64,              // 0.0-1.0
        action: String,          // "logged", "blocked"
        categories: Vec<String>, // matched pattern categories
        content: String,         // full input content for forensics
    },
    /// L3: HMAC boundary validation failure.
    BoundaryFailure {
        event_id: String,
        timestamp: String,
        correlation_id: String,
        zone: String,            // "static", "conditional", "dynamic"
        source: String,          // provenance
        action: String,          // "stripped"
        content: String,         // corrupted content for forensics
    },
    /// L4: Output screening detection.
    OutputScreening {
        event_id: String,
        timestamp: String,
        correlation_id: String,
        detection_type: String,  // "credential_leak", "injection_relay"
        tool_name: String,
        action: String,          // "redacted", "blocked"
        content: String,         // screened content
    },
    /// L5: HITL confirmation prompt.
    HitlPrompt {
        event_id: String,
        timestamp: String,
        correlation_id: String,
        tool_name: String,
        risk_level: String,      // "low", "medium", "high"
        action: String,          // "approved", "denied", "timeout"
        session_id: String,
    },
}
```

### Pattern 4: Config Structure (follows figment + TOML pattern)
**What:** Nested TOML config with `deny_unknown_fields`, Option<T> for optional sections
**When to use:** All injection defense configuration
**Example:**
```rust
// Source: blufio-config::model pattern (ClassificationConfig as model)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InjectionDefenseConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default)]
    pub dry_run: bool,

    #[serde(default)]
    pub input_detection: InputDetectionConfig,

    #[serde(default)]
    pub hmac_boundaries: HmacBoundaryConfig,

    #[serde(default)]
    pub output_screening: OutputScreeningConfig,

    #[serde(default)]
    pub hitl: HitlConfig,
}

fn default_enabled() -> bool { true }
```

### Anti-Patterns to Avoid
- **Single crate bloat:** Do NOT add injection defense to `blufio-security`. Different concerns warrant separate crate (`blufio-injection`), as specified in CONTEXT.md.
- **Mutable shared state for patterns:** Do NOT use `RwLock<Vec<Regex>>` for pattern storage. Use `LazyLock<RegexSet>` for compiled defaults + separate compiled custom patterns at startup.
- **Blocking HMAC validation in hot path:** HMAC-SHA256 is <1us for typical messages. No async needed. But DO NOT hold locks across HMAC operations.
- **Leaking detection details to users:** Per CONTEXT.md, blocked messages get generic "I can't process this message." NEVER reveal which patterns matched, score, or layer that caught it.
- **Coupling layers:** Each layer acts independently with its own action. No unified verdict score. Cross-layer escalation is additive (L1 flag raises L4/L5 strictness) but never short-circuits.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HMAC-SHA256 signing | Custom HMAC implementation | `ring::hmac::sign` / `ring::hmac::verify` | Timing-safe comparison, audited crypto, already in workspace |
| HKDF key derivation | Manual extract-then-expand | `ring::hkdf::Salt::extract` + `Prk::expand` | RFC 5869 compliant, handles edge cases |
| Regex compilation | Dynamic regex compilation per-request | `LazyLock<RegexSet>` compiled once | RegexSet is O(n) compile time but O(1) match time; compile once at startup |
| Credential pattern matching | Separate regex engines for L4 vs redaction | Shared pattern registry extending REDACTION_PATTERNS | CONTEXT.md explicitly says "shared pattern registry for log redaction and output screening" |
| Base64 encoding | Manual byte manipulation | `base64` crate or `ring`'s built-in | Handles padding, URL-safe variants, edge cases |
| Timing-safe comparison | `==` on HMAC tags | `ring::hmac::verify` | Constant-time comparison prevents timing side channels |

**Key insight:** The existing codebase already contains proven patterns for every component of this system. L1 mirrors PII RegexSet, L3 uses ring (already in vault), L4 extends RedactingWriter patterns, EventBus/Prometheus/Config all have established patterns. The implementation risk is in integration, not in novel algorithms.

## Common Pitfalls

### Pitfall 1: RegexSet Index Mismatch
**What goes wrong:** RegexSet and individual Regex vectors get out of sync, causing wrong pattern IDs
**Why it happens:** Separate construction of RegexSet and individual patterns
**How to avoid:** Single source of truth array (exactly as PII module does with `PATTERNS` static array). Both RegexSet and individual regexes built from same array.
**Warning signs:** Unit test with known pattern returns wrong category

### Pitfall 2: HMAC Key Scope Confusion
**What goes wrong:** Using a global HMAC key instead of per-session key allows cross-session replay attacks
**Why it happens:** Simpler to derive one key at startup
**How to avoid:** HKDF with session_id as salt, master_key as IKM. New session = new derived key. Key stored in SessionActor state.
**Warning signs:** Boundary tokens from session A validate in session B

### Pitfall 3: HMAC Token in LLM Context
**What goes wrong:** Boundary tokens leak into LLM context, confusing the model
**Why it happens:** Forgetting to strip tokens before sending to provider
**How to avoid:** CONTEXT.md specifies "transparent markers -- verified and stripped before sending to LLM." Strip function MUST run after validation, before ProviderRequest construction.
**Warning signs:** LLM responses reference "BLUF-ZONE" strings

### Pitfall 4: Blocking Legitimate Input
**What goes wrong:** Common phrases like "ignore previous" in normal conversation trigger false positives
**Why it happens:** Overly broad regex patterns
**How to avoid:** log-not-block default mode (INJC-02). Only block at >0.95 score. Scoring weights severity + match count + position (early in message = higher risk). Operators tune via audit log review.
**Warning signs:** Non-malicious messages getting blocked in testing

### Pitfall 5: Output Screening Race with Streaming
**What goes wrong:** Credential appears in streamed output before screening catches it
**Why it happens:** L4 screens tool call arguments, not streamed text
**How to avoid:** CONTEXT.md is explicit: "Stream text to user in real-time, buffer tool call arguments for screening." L4 only intercepts before tool EXECUTION, not before user sees streamed text. This is by design.
**Warning signs:** Attempting to buffer streamed text (wrong approach per spec)

### Pitfall 6: HITL Blocking Agent Loop
**What goes wrong:** Waiting for HITL confirmation blocks the entire session actor
**Why it happens:** Synchronous wait for user response in async context
**How to avoid:** Use tokio timeout with `tokio::time::timeout(Duration::from_secs(60), ...)`. Auto-deny on timeout. The agent should continue processing after denial with "Tool [X] was blocked."
**Warning signs:** Session hangs when no HITL response arrives

### Pitfall 7: Cross-Crate Dependency Cycle
**What goes wrong:** blufio-injection depends on blufio-agent for SessionActor types
**Why it happens:** Wanting to embed injection logic directly in SessionActor
**How to avoid:** blufio-injection provides trait-based scanning functions. blufio-agent calls into blufio-injection, not the reverse. blufio-injection depends on blufio-core (types), blufio-config (config), blufio-bus (events), blufio-vault (key access).
**Warning signs:** Circular dependency errors in cargo build

### Pitfall 8: Custom Regex Startup Crash
**What goes wrong:** Invalid custom regex in TOML config crashes server at startup
**Why it happens:** Compiling user-provided regex without error handling
**How to avoid:** CONTEXT.md specifies "Custom regex validated at startup (compile check, reject invalid with warning)" and "warn and use defaults for invalid values. Server still starts."
**Warning signs:** Server fails to start with regex compilation error

## Code Examples

Verified patterns from official sources and existing codebase:

### HKDF Key Derivation with ring 0.17
```rust
// Source: docs.rs/ring/0.17.14/ring/hkdf/
use ring::{hkdf, hmac};

pub struct BoundaryKeyDeriver {
    master_key: [u8; 32],
}

impl BoundaryKeyDeriver {
    pub fn derive_session_key(&self, session_id: &str) -> hmac::Key {
        // Use session_id as salt for domain separation
        let salt = hkdf::Salt::new(
            hkdf::HKDF_SHA256,
            session_id.as_bytes(),
        );
        // Extract PRK from master key
        let prk = salt.extract(&self.master_key);
        // Expand with "hmac-boundary" info context
        let okm = prk
            .expand(&[b"hmac-boundary"], &ring::hmac::HMAC_SHA256)
            .expect("HMAC key length is valid for HKDF");
        // NOTE: ring::hkdf::Okm implements ring::hmac::KeyValue,
        // but for simplicity extract bytes manually
        let mut key_bytes = [0u8; 32];
        okm.fill(&mut key_bytes).expect("32 bytes fits HMAC-SHA256 key");
        hmac::Key::new(hmac::HMAC_SHA256, &key_bytes)
    }
}
```

### HMAC Sign and Verify with ring 0.17
```rust
// Source: docs.rs/ring/0.17.14/ring/hmac/
use ring::hmac;

/// Sign content and return base64-encoded tag.
pub fn sign_zone_content(key: &hmac::Key, content: &str) -> String {
    let tag = hmac::sign(key, content.as_bytes());
    // Use URL-safe base64 without padding for compact tokens
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(tag.as_ref())
}

/// Verify a boundary token's HMAC tag against content.
pub fn verify_zone_content(
    key: &hmac::Key,
    content: &str,
    encoded_tag: &str,
) -> bool {
    let tag_bytes = match base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded_tag)
    {
        Ok(b) => b,
        Err(_) => return false,
    };
    // ring::hmac::verify uses constant-time comparison
    hmac::verify(key, content.as_bytes(), &tag_bytes).is_ok()
}
```

### Injection Scoring Algorithm
```rust
// Source: CONTEXT.md specification + PII module pattern
/// Calculate injection confidence score from matched patterns.
///
/// Score = weighted sum of pattern severities + positional bonus.
/// Clamped to [0.0, 1.0].
pub fn calculate_score(
    matches: &[InjectionMatch],
    input_length: usize,
) -> f64 {
    if matches.is_empty() {
        return 0.0;
    }

    let mut score = 0.0;

    for m in matches {
        // Base severity per pattern (0.1 - 0.5)
        score += m.severity;

        // Positional bonus: patterns at start of message are more suspicious
        let position_ratio = 1.0 - (m.span.start as f64 / input_length.max(1) as f64);
        score += position_ratio * 0.1; // up to 0.1 bonus for early position
    }

    // Match count bonus: multiple patterns = more suspicious
    if matches.len() > 1 {
        score += (matches.len() - 1) as f64 * 0.1;
    }

    score.clamp(0.0, 1.0)
}
```

### Adding SecurityEvent to BusEvent (follows ClassificationEvent pattern)
```rust
// Source: blufio-bus::events pattern
// In blufio-bus/src/events.rs, add to BusEvent enum:
pub enum BusEvent {
    // ... existing 14 variants ...

    /// Security events (injection defense: detection, boundary, screening, HITL).
    Security(SecurityEvent),
}

// In event_type_string():
BusEvent::Security(SecurityEvent::InputDetection { .. }) => "security.input_detection",
BusEvent::Security(SecurityEvent::BoundaryFailure { .. }) => "security.boundary_failure",
BusEvent::Security(SecurityEvent::OutputScreening { .. }) => "security.output_screening",
BusEvent::Security(SecurityEvent::HitlPrompt { .. }) => "security.hitl_prompt",
```

### HITL Confirmation Flow
```rust
// Source: CONTEXT.md specification
use tokio::time::{timeout, Duration};

pub struct HitlManager {
    timeout_secs: u64,
    safe_tools: Vec<String>,
    session_approvals: HashMap<String, HashSet<String>>, // session_id -> approved tool types
    pending_count: usize,
    max_pending: usize,
}

impl HitlManager {
    pub async fn check_tool_execution(
        &mut self,
        session_id: &str,
        tool_name: &str,
        tool_args_summary: &str,
        channel_supports_reply: bool,
    ) -> HitlDecision {
        // Safe tools always auto-approved
        if self.safe_tools.contains(&tool_name.to_string()) {
            return HitlDecision::Approved;
        }

        // Already approved this tool type in this session
        if let Some(approved) = self.session_approvals.get(session_id) {
            if approved.contains(tool_name) {
                return HitlDecision::Approved;
            }
        }

        // Non-interactive channel: auto-deny
        if !channel_supports_reply {
            return HitlDecision::Denied("non-interactive channel".into());
        }

        // Max pending check
        if self.pending_count >= self.max_pending {
            return HitlDecision::Denied("max pending confirmations reached".into());
        }

        self.pending_count += 1;
        // Send confirmation message and wait for reply
        // ... channel-specific delivery ...
        // Auto-deny on timeout
        HitlDecision::Pending
    }
}
```

### CLI Injection Test Command
```rust
// Source: blufio/src/main.rs pattern (AuditCommands as model)
#[derive(Subcommand, Debug)]
enum InjectionCommands {
    /// Test a text string against injection patterns.
    Test {
        /// Text to scan for injection patterns.
        text: String,
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
        /// Disable colored output.
        #[arg(long, alias = "no-color")]
        plain: bool,
    },
    /// Show injection defense status.
    Status {
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show effective injection defense config.
    Config {
        /// Output as structured JSON.
        #[arg(long)]
        json: bool,
    },
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Instruction delimiters only | HMAC-signed content zones | 2024-2025 | Cryptographic verification prevents delimiter spoofing |
| Block all detected injections | Log-not-block with confidence scoring | 2025 | Reduces false positives, operators tune from audit logs |
| Single detection layer | Defense-in-depth (5 layers) | 2024-2025 OWASP Top 10 | No single layer can prevent all attacks; layering is standard |
| Static allow/deny lists | Configurable thresholds + custom patterns | 2025-2026 | Operators adapt to their specific threat model |
| Trust all LLM output | Output screening before tool execution | 2025-2026 | Prevents indirect injection via LLM output relay |

**Deprecated/outdated:**
- Simple keyword blocking without scoring: too many false positives for production use
- Trusting tool descriptions from external MCP servers: OWASP recommends sanitization (already done in `blufio-mcp-client::sanitize`)

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` + `proptest` for property tests |
| Config file | Per-crate `Cargo.toml` `[dev-dependencies]` |
| Quick run command | `cargo test -p blufio-injection` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INJC-01 | L1 pattern classifier detects injection patterns with 0.0-1.0 scoring | unit | `cargo test -p blufio-injection -- classifier` | Wave 0 |
| INJC-01 | L1 RegexSet fast path returns no match for clean input | unit | `cargo test -p blufio-injection -- classifier::clean` | Wave 0 |
| INJC-02 | L1 log-not-block default mode: detections logged, not blocked below threshold | unit | `cargo test -p blufio-injection -- classifier::log_mode` | Wave 0 |
| INJC-02 | L1 blocks at >0.95 confidence | unit | `cargo test -p blufio-injection -- classifier::blocking` | Wave 0 |
| INJC-03 | HMAC boundary tokens sign/verify roundtrip | unit | `cargo test -p blufio-injection -- boundary::roundtrip` | Wave 0 |
| INJC-03 | HMAC boundary strip-before-LLM | unit | `cargo test -p blufio-injection -- boundary::strip` | Wave 0 |
| INJC-03 | Tampered zone content fails HMAC verification | unit | `cargo test -p blufio-injection -- boundary::tamper` | Wave 0 |
| INJC-04 | L4 detects credential patterns in tool arguments | unit | `cargo test -p blufio-injection -- output_screen::credentials` | Wave 0 |
| INJC-04 | L4 detects injection relay in LLM output | unit | `cargo test -p blufio-injection -- output_screen::relay` | Wave 0 |
| INJC-05 | HITL approval flow with timeout | unit | `cargo test -p blufio-injection -- hitl::timeout` | Wave 0 |
| INJC-05 | HITL safe tools auto-approved | unit | `cargo test -p blufio-injection -- hitl::safe_tools` | Wave 0 |
| INJC-06 | MCP tool output scanned with L1 patterns | integration | `cargo test -p blufio-injection -- integration::mcp` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-injection`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full workspace test suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/blufio-injection/` -- entire crate does not exist yet
- [ ] `crates/blufio-injection/Cargo.toml` -- new crate manifest
- [ ] `crates/blufio-injection/src/lib.rs` -- crate root with re-exports
- [ ] Attack corpus test data (known injection patterns for validation)
- [ ] `base64` dependency needs adding to workspace Cargo.toml (if used for token encoding)

## Open Questions

1. **ring::hkdf Okm to hmac::Key conversion**
   - What we know: ring 0.17 has `hkdf::Okm` with `fill()` method to extract raw bytes, and `hmac::Key::new()` accepts raw bytes
   - What's unclear: Whether ring provides a more direct Okm -> hmac::Key path without intermediate byte extraction (the `KeyType` trait might allow this)
   - Recommendation: Use `fill()` to extract 32 bytes, then construct `hmac::Key::new()`. Simple and works. Verify with compilation.

2. **HITL Channel Delivery Mechanism**
   - What we know: CONTEXT.md specifies "inline message + reply in same conversation"
   - What's unclear: How to get a reply from the user within `execute_tools()` -- the current ChannelAdapter trait is unidirectional (send message to channel)
   - Recommendation: Add a `request_confirmation()` method to ChannelAdapter trait or use a separate confirmation channel (e.g., oneshot channel per pending confirmation). The SessionActor already has access to the channel multiplexer.

3. **Boundary Token Position in Assembled Context**
   - What we know: Boundaries apply to "final assembled context only, not prompt cache layer"
   - What's unclear: Exact insertion points -- should tokens wrap each zone's content in the ProviderRequest, or wrap each individual ProviderMessage?
   - Recommendation: Wrap at the zone level (one boundary per zone: static system blocks, conditional block, dynamic messages block). Zone-level is sufficient for detecting cross-zone injection.

4. **base64 vs hex for HMAC Tags**
   - What we know: Tags are 32 bytes (HMAC-SHA256). Base64 = 43 chars, hex = 64 chars
   - What's unclear: Whether base64 adds a dependency or if ring/hex already handles this
   - Recommendation: `hex` crate (0.4) is already in workspace. Use hex encoding for simplicity. 64 chars is acceptable for tokens that get stripped.

## Sources

### Primary (HIGH confidence)
- [ring 0.17.14 hkdf module](https://docs.rs/ring/0.17.14/ring/hkdf/) -- HKDF_SHA256 API, Salt/Prk/Okm types
- [ring 0.17.14 hmac module](https://docs.rs/ring/0.17.14/ring/hmac/) -- HMAC_SHA256 sign/verify API
- `blufio-security::pii` (crates/blufio-security/src/pii.rs) -- RegexSet two-phase detection pattern, single source of truth array
- `blufio-security::redact` (crates/blufio-security/src/redact.rs) -- RedactingWriter, REDACTION_PATTERNS, credential regex patterns
- `blufio-bus::events` (crates/blufio-bus/src/events.rs) -- BusEvent 14-variant pattern, String fields for cross-crate safety
- `blufio-vault::crypto` / `blufio-vault::kdf` -- ring-based crypto, Argon2id KDF (model for HKDF)
- `blufio-mcp-client::sanitize` -- MCP description sanitization (existing injection defense layer)
- `blufio-agent::session` -- SessionActor handle_message (L1 insertion), execute_tools (L4/L5 insertion)
- `blufio-context::lib` -- ContextEngine::assemble (L3 insertion point)
- `blufio-config::model` -- Config pattern with deny_unknown_fields, McpServerEntry.trusted flag
- Phase 57 CONTEXT.md -- All locked decisions and design specifications

### Secondary (MEDIUM confidence)
- [OWASP LLM Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html) -- Canonical injection regex patterns, defense-in-depth architecture
- [OWASP LLM01:2025 Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) -- Current threat classification and mitigation strategies

### Tertiary (LOW confidence)
- General prompt injection defense patterns from web search -- validated against OWASP and CONTEXT.md specifications

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace (ring, regex, hmac, sha2), APIs verified via official docs
- Architecture: HIGH -- every pattern mirrors existing proven codebase modules (PII for L1, vault crypto for L3, redact for L4, EventBus for events)
- Pitfalls: HIGH -- identified from PII module development history (Phase 53 decisions document RegexSet index mismatch explicitly)
- Integration points: HIGH -- specific line numbers and method signatures verified from source code
- HITL delivery mechanism: MEDIUM -- channel reply mechanism needs design; existing ChannelAdapter is outbound-only

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable domain, no fast-moving dependencies)
