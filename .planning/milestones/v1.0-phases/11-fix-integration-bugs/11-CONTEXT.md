# Phase 11: Fix Critical Integration Bugs - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix 4 cross-phase integration bugs identified by the v1.0 milestone audit: tool content block serialization (P0), vault startup wiring (P1), keypair auth gateway wiring (P2), and model router bypass in tool follow-up (P3). No new features — only wiring fixes to make existing code work correctly together.

</domain>

<decisions>
## Implementation Decisions

### P0: Tool Content Block Serialization
- Add `ToolUse { id, name, input }` and `ToolResult { tool_use_id, content, is_error }` as first-class variants to `blufio-core::ContentBlock` enum
- Tool input stored as `serde_json::Value` — matches Anthropic API format directly
- Tool result content is a single `String` (text-only) — covers 99% of cases, can extend to multi-block later
- Include `is_error: Option<bool>` on ToolResult for proper error signaling to the LLM
- Update `blufio-anthropic` converter (`convert_content_blocks`) to map ToolUse/ToolResult to correct Anthropic API content block types
- Fix `blufio-agent/src/lib.rs:399-415` to emit structured ToolUse/ToolResult blocks instead of JSON-serialized Text blocks

### P1: Vault Startup Wiring
- Call `vault_startup_check()` early in `serve.rs`, before provider initialization — fail fast if vault is locked
- Silent no-op when no vault secrets exist (most users use config/env for API keys)
- If vault check fails (wrong passphrase, corrupted), abort serve entirely with clear error message
- Use existing `get_vault_passphrase()` for interactive prompt; support env var fallback for headless/daemon mode

### P2: Keypair Auth Gateway Wiring
- Extend existing `auth_middleware()` in `blufio-gateway/src/auth.rs` — add keypair signature verification as second auth method alongside bearer token
- Check bearer_token first, then keypair signature; if neither configured, reject (fail-closed)
- When gateway enabled but no auth configured, refuse to start — no accidental open gateways
- Use signed request body + timestamp for replay prevention (reject if timestamp > 60s old), matching existing `SignedAgentMessage` pattern
- Auto-load device keypair from vault during startup — no manual public key config needed

### P3: Model Router Bypass in Tool Follow-up
- Store the initially-routed model in session/conversation state — tool follow-ups reuse the same model (consistent mid-conversation)
- Replace hardcoded `self.config.anthropic.default_model` at `lib.rs:432` with the stored routed model
- `ModelRouter::select()` already returns default_model when routing is disabled — no explicit check needed
- Debug-level logging when routed model is used for tool follow-ups

### Claude's Discretion
- Exact field naming conventions for new ContentBlock variants (follow existing enum style)
- Error message wording for vault/auth failures
- Whether to add integration tests for each fix or rely on existing test infrastructure

</decisions>

<specifics>
## Specific Ideas

- Security-first approach throughout: fail-closed on auth, abort on vault failure, no silent degradation
- All 4 bugs are wiring issues — the code exists but isn't connected. Fixes should be surgical, not architectural changes
- P0 is the highest priority since it breaks all multi-turn tool conversations

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ContentBlock` enum (`blufio-core/src/types.rs:126`): Currently has Text and Image — adding ToolUse/ToolResult here
- `convert_content_blocks()` (`blufio-anthropic/src/lib.rs:411`): Maps ContentBlock to ApiContentBlock — needs new variant handling
- `vault_startup_check()` (`blufio-vault/src/lib.rs:17`): Already exported, just needs to be called
- `get_vault_passphrase()` (`blufio-vault/src/prompt.rs`): Existing interactive prompt function
- `KeypairAuthAdapter` (`blufio-auth-keypair/src/lib.rs:26`): Full adapter with Ed25519 verification, just not wired
- `SignedAgentMessage` / `AgentMessage` (`blufio-auth-keypair/src/message.rs`): Existing signed message types for request verification
- `auth_middleware()` (`blufio-gateway/src/auth.rs:28`): Bearer token check — extend with keypair verification
- `Arc<ModelRouter>` (`blufio-agent/src/lib.rs:64`): Already held by agent — just not used in tool follow-up path

### Established Patterns
- Content block pattern: tagged enum with `#[serde(tag = "type")]` — new variants follow same pattern
- Gateway auth: axum middleware with `Extension<AuthConfig>` — extend AuthConfig with keypair option
- Router integration: Agent holds `Arc<ModelRouter>`, calls `router.select()` for initial requests — follow-ups should match

### Integration Points
- `blufio-agent/src/lib.rs:399-415`: Tool loop where Text blocks replace structured blocks (P0 fix location)
- `blufio-agent/src/lib.rs:432`: Hardcoded default_model for follow-up (P3 fix location)
- `blufio/src/serve.rs`: Startup sequence where vault check needs insertion (P1 fix location)
- `blufio-gateway/src/auth.rs`: Auth middleware to extend (P2 fix location)
- `blufio-gateway/src/server.rs:57`: GatewayConfig bearer_token field — may need keypair auth config addition

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 11-fix-integration-bugs*
*Context gathered: 2026-03-01*
