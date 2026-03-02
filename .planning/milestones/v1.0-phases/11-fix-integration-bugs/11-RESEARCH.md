# Phase 11: Fix Critical Integration Bugs - Research

**Researched:** 2026-03-01
**Domain:** Cross-phase integration wiring (Rust, async, axum middleware, serde)
**Confidence:** HIGH

## Summary

Phase 11 addresses 4 cross-phase integration bugs identified by the v1.0 milestone audit. All 4 are **wiring issues** — the code exists in isolation but is not connected correctly at integration points. No new architecture or libraries are needed; fixes are surgical insertions and modifications to existing code.

The bugs span 4 crates: `blufio-core` (content block types), `blufio-agent` (tool loop serialization + model routing), `blufio-vault` (startup check), and `blufio-gateway` (auth middleware). Each fix is independent and can be developed and tested in parallel, though P0 (tool content blocks) is the highest priority since it breaks all multi-turn tool conversations.

**Primary recommendation:** Fix all 4 bugs as independent plans in a single wave, since they have no interdependencies. Each fix touches 2-3 files and should be testable in isolation.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **P0: Tool Content Block Serialization** — Add `ToolUse { id, name, input }` and `ToolResult { tool_use_id, content, is_error }` as first-class variants to `blufio-core::ContentBlock` enum. Tool input as `serde_json::Value`, tool result content as single `String`, include `is_error: Option<bool>`. Update `blufio-anthropic` converter and fix `blufio-agent/src/lib.rs:399-415`.
- **P1: Vault Startup Wiring** — Call `vault_startup_check()` early in `serve.rs`, before provider initialization. Silent no-op when no vault secrets exist. Abort serve entirely on failure with clear error message. Support env var fallback for headless mode.
- **P2: Keypair Auth Gateway Wiring** — Extend `auth_middleware()` with keypair signature verification as second auth method. Check bearer_token first, then keypair. When gateway enabled but no auth configured, refuse to start. Use signed request body + timestamp for replay prevention (reject >60s old). Auto-load device keypair from vault during startup.
- **P3: Model Router Bypass in Tool Follow-up** — Store initially-routed model in session/conversation state. Replace hardcoded `self.config.anthropic.default_model` at `lib.rs:432` with stored routed model. Debug-level logging for tool follow-ups.

### Claude's Discretion
- Exact field naming conventions for new ContentBlock variants (follow existing enum style)
- Error message wording for vault/auth failures
- Whether to add integration tests for each fix or rely on existing test infrastructure

### Deferred Ideas (OUT OF SCOPE)
None — all 4 bugs are in scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LLM-05 | Model router classifies query complexity and routes to Haiku/Sonnet/Opus | P3 fix ensures routed model is used for tool follow-ups, completing the model routing integration |
| SEC-02 | Device keypair authentication required — no optional auth mode | P2 fix wires `KeypairAuthAdapter` into gateway auth middleware, making keypair auth functional |
| SEC-03 | AES-256-GCM encrypted credential vault stores all API keys and bot tokens | P1 fix calls `vault_startup_check()` during startup, making vault-stored secrets accessible |
</phase_requirements>

## Standard Stack

### Core (No New Dependencies)
| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| serde / serde_json | existing | ContentBlock serialization with `#[serde(tag = "type")]` | Already in Cargo.toml |
| axum | existing | Gateway middleware extension for keypair auth | Already in blufio-gateway |
| ed25519-dalek | existing | Keypair signature verification in auth middleware | Already in blufio-auth-keypair |
| chrono | existing | Timestamp parsing for replay prevention | Already in blufio-auth-keypair |
| tokio-rusqlite | existing | Vault DB connection for startup check | Already in blufio-vault |
| tracing | existing | Debug logging for model routing in follow-ups | Already everywhere |

**No new crate dependencies are required for any of the 4 fixes.**

## Architecture Patterns

### Pattern 1: Tagged Enum Extension (P0)
**What:** `ContentBlock` uses `#[serde(tag = "type")]` for JSON serialization. New variants follow the same pattern.
**Existing pattern in `blufio-core/src/types.rs:124-137`:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source_type: String, media_type: String, data: String },
}
```
**New variants follow identical style:**
```rust
#[serde(rename = "tool_use")]
ToolUse { id: String, name: String, input: serde_json::Value },
#[serde(rename = "tool_result")]
ToolResult { tool_use_id: String, content: String, #[serde(skip_serializing_if = "Option::is_none")] is_error: Option<bool> },
```

### Pattern 2: Axum State-Based Middleware (P2)
**What:** Gateway auth uses `State<AuthConfig>` extracted by axum middleware.
**Existing pattern in `blufio-gateway/src/auth.rs:28-48`:**
```rust
pub async fn auth_middleware(
    State(auth): State<AuthConfig>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> { ... }
```
**Extension:** Add `keypair_verifier` field to `AuthConfig`, check after bearer token fallthrough.

### Pattern 3: Session State Storage (P3)
**What:** `SessionActor` already stores `last_routing_decision: Option<RoutingDecision>`. The routed model from `decision.actual_model` should be reused in tool follow-up requests.
**Existing in `session.rs:253-276`:** Router is called, decision stored, model extracted.
**Bug location in `lib.rs:432`:** Follow-up hardcodes `self.config.anthropic.default_model` instead of using the stored decision's model.

### Anti-Patterns to Avoid
- **Don't add new crate dependencies** — all needed code exists
- **Don't restructure the auth middleware** — extend AuthConfig, don't replace the middleware function
- **Don't change the RoutingDecision struct** — just read from the existing stored decision
- **Don't make vault check interactive during tests** — use BLUFIO_VAULT_KEY env var

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Signature verification | Custom crypto | `DeviceKeypair::verify_strict()` | Already implemented, tested, uses ed25519-dalek best practices |
| Replay prevention | Custom nonce tracking | Timestamp-based rejection (>60s) | Simple, stateless, matches existing `AgentMessage.timestamp` pattern |
| Vault passphrase prompt | Custom stdin reader | `get_vault_passphrase()` | Already handles TTY detection + env var fallback |

## Common Pitfalls

### Pitfall 1: ContentBlock Serde Compatibility
**What goes wrong:** Adding new variants to `ContentBlock` breaks existing serialized data in SQLite storage.
**Why it happens:** Messages stored in DB contain serialized `ContentBlock` values.
**How to avoid:** New variants use `#[serde(rename = "...")]` matching Anthropic API format. Existing stored messages only contain `text` and `image` variants, so deserialization remains backward-compatible. The `#[serde(tag = "type")]` discriminator handles unknown variants gracefully if old code reads new data.
**Warning signs:** Deserialization errors when loading old conversations.

### Pitfall 2: Auth Middleware Order of Operations
**What goes wrong:** Keypair auth check runs before bearer token check, causing performance degradation (signature verification is more expensive than string comparison).
**Why it happens:** Checking order matters for both security and performance.
**How to avoid:** Check bearer_token first (fast path, string comparison), then keypair signature verification (slow path, crypto operation). If neither configured and gateway is enabled, refuse to start.

### Pitfall 3: Vault Startup Position
**What goes wrong:** Vault check runs too late (after provider init), so API keys from vault aren't available when Anthropic provider tries to read them.
**Why it happens:** `vault_startup_check()` needs to run before any component that reads secrets.
**How to avoid:** Insert vault check immediately after tracing init and plugin registry, before storage/provider initialization. The `vault_startup_check()` function already returns `Option<Vault>` — `None` means no vault exists (silent no-op), `Some(vault)` means vault is unlocked and secrets are available.

### Pitfall 4: Model Routing in Follow-ups with Routing Disabled
**What goes wrong:** When routing is disabled (`config.routing.enabled == false`), there's no stored routing decision, but follow-up still needs a model.
**Why it happens:** `last_routing_decision` is `None` when routing is disabled.
**How to avoid:** When routing is disabled, `default_model` is correct for both initial and follow-up requests. Only override follow-up model when routing IS enabled and a decision exists.

### Pitfall 5: Replay Prevention Clock Skew
**What goes wrong:** Legitimate requests rejected because server and client clocks differ.
**Why it happens:** 60-second window is tight for cross-network communication.
**How to avoid:** The 60-second window is for same-device inter-agent communication (not cross-network), so clock skew is negligible. Use `chrono::Utc::now()` for both sides.

## Code Examples

### P0: ContentBlock Variant Addition
```rust
// In blufio-core/src/types.rs, add to ContentBlock enum:
/// Tool use content block (assistant requests tool execution).
#[serde(rename = "tool_use")]
ToolUse {
    id: String,
    name: String,
    input: serde_json::Value,
},
/// Tool result content block (user provides tool execution result).
#[serde(rename = "tool_result")]
ToolResult {
    tool_use_id: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
},
```

### P0: Converter Update in blufio-anthropic
```rust
// In convert_content_blocks(), add new match arms:
ContentBlock::ToolUse { id, name, input } => ApiContentBlock::ToolUse {
    id: id.clone(),
    name: name.clone(),
    input: input.clone(),
},
ContentBlock::ToolResult { tool_use_id, content, is_error } => ApiContentBlock::ToolResult {
    tool_use_id: tool_use_id.clone(),
    content: content.clone(),
    is_error: *is_error,
},
```

### P0: Fix Tool Loop in blufio-agent (lib.rs:399-415)
```rust
// Replace JSON-serialized Text blocks with structured ContentBlocks:
// Assistant message with ToolUse blocks:
messages.push(ProviderMessage {
    role: "assistant".to_string(),
    content: assistant_content_blocks.iter().map(|block| {
        ContentBlock::ToolUse {
            id: block.id.clone(),
            name: block.name.clone(),
            input: block.input.clone(),
        }
    }).collect(),
});

// User message with ToolResult blocks:
messages.push(ProviderMessage {
    role: "user".to_string(),
    content: tool_results.iter().map(|result| {
        ContentBlock::ToolResult {
            tool_use_id: result.tool_use_id.clone(),
            content: result.content.clone(),
            is_error: result.is_error,
        }
    }).collect(),
});
```

### P1: Vault Startup Check in serve.rs
```rust
// Insert after plugin registry init, before storage init:
// Vault startup check (SEC-03): unlock vault if it exists.
let _vault = blufio_vault::vault_startup_check(
    tokio_rusqlite::Connection::open(&config.storage.database_path).await
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?,
    &config.vault,
).await.map_err(|e| {
    error!(error = %e, "vault startup check failed");
    eprintln!("error: vault exists but cannot be unlocked. Set BLUFIO_VAULT_KEY env var or provide passphrase interactively.");
    e
})?;
```

### P3: Fix Tool Follow-up Model
```rust
// In lib.rs around line 432, replace:
//   model: self.config.anthropic.default_model.clone(),
// With:
let follow_up_model = self.sessions.get(&session_id)
    .and_then(|actor| actor.last_routing_decision())
    .map(|d| d.actual_model.clone())
    .unwrap_or_else(|| self.config.anthropic.default_model.clone());

let follow_up_request = ProviderRequest {
    model: follow_up_model,
    // ... rest unchanged
};
```

## Integration Points Summary

| Bug | Source File(s) | Line(s) | Fix Type |
|-----|---------------|---------|----------|
| P0 | `blufio-core/src/types.rs` | 124-137 | Add 2 enum variants |
| P0 | `blufio-anthropic/src/lib.rs` | 411-437 | Add 2 match arms to converter |
| P0 | `blufio-agent/src/lib.rs` | 399-415 | Replace JSON Text with structured blocks |
| P1 | `blufio/src/serve.rs` | ~84 (after plugin init) | Insert vault_startup_check call |
| P2 | `blufio-gateway/src/auth.rs` | 18-48 | Extend AuthConfig + auth_middleware |
| P2 | `blufio-gateway/src/server.rs` | 57 | Add keypair config to ServerConfig |
| P2 | `blufio/src/serve.rs` | ~218-236 (gateway section) | Wire keypair into gateway config |
| P3 | `blufio-agent/src/lib.rs` | 432 | Use stored routing decision model |

## Sources

### Primary (HIGH confidence)
- Codebase inspection: `blufio-core/src/types.rs` — ContentBlock enum definition
- Codebase inspection: `blufio-anthropic/src/types.rs` — ApiContentBlock already has ToolUse/ToolResult variants
- Codebase inspection: `blufio-agent/src/lib.rs:399-415` — Tool loop bug location confirmed
- Codebase inspection: `blufio-agent/src/lib.rs:432` — Hardcoded default_model confirmed
- Codebase inspection: `blufio-vault/src/migration.rs:123-137` — vault_startup_check signature and behavior
- Codebase inspection: `blufio-gateway/src/auth.rs` — Current auth middleware implementation
- Codebase inspection: `blufio-auth-keypair/src/lib.rs` — KeypairAuthAdapter ready to wire
- Codebase inspection: `blufio-agent/src/session.rs:253-276` — Router usage and decision storage

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies needed, all code exists
- Architecture: HIGH — patterns are established, fixes follow existing conventions
- Pitfalls: HIGH — bugs are well-characterized by milestone audit, codebase inspection confirms

**Research date:** 2026-03-01
**Valid until:** 2026-03-31 (stable — bug fixes to existing code)
