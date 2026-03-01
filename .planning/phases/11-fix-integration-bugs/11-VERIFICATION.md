# Phase 11 Verification: Fix Critical Integration Bugs

**Phase:** 11-fix-integration-bugs
**Verified:** 2026-03-01
**Requirements:** LLM-05, SEC-02, SEC-03

## Success Criteria Verification

### SC-1: Multi-turn tool conversations produce correct ToolUse/ToolResult content blocks -- no 400 API errors
**Status:** PASS

**Evidence:**
- `ContentBlock` enum in `crates/blufio-core/src/types.rs` now has 4 variants: `Text`, `Image`, `ToolUse`, `ToolResult`
- `convert_content_blocks` in `crates/blufio-anthropic/src/lib.rs` maps `ContentBlock::ToolUse` to `ApiContentBlock::ToolUse` and `ContentBlock::ToolResult` to `ApiContentBlock::ToolResult`
- Agent tool loop in `crates/blufio-agent/src/lib.rs` constructs `ProviderMessage` with structured `ContentBlock::ToolUse` (assistant role) and `ContentBlock::ToolResult` (user role) instead of JSON-serialized text blocks
- The `is_error` field is properly converted from `bool` to `Option<bool>` matching the Anthropic API expectation

### SC-2: vault_startup_check() is called during blufio serve startup -- vault-backed API keys are usable
**Status:** PASS

**Evidence:**
- `blufio_vault::vault_startup_check(vault_conn, &config.vault)` is called in `crates/blufio/src/serve.rs` at line 92, after plugin registry init and before storage/provider init
- Three-way match handles `Ok(Some(_vault))` (success), `Ok(None)` (no vault -- silent), and `Err(e)` (abort with clear error message)
- Error path returns `Err(e)` which aborts the serve command entirely
- Vault is checked before `AnthropicProvider::new` ensuring secrets are available for provider initialization

### SC-3: KeypairAuthAdapter is wired into gateway HTTP auth middleware -- unauthenticated requests are rejected when keypair auth is configured
**Status:** PASS

**Evidence:**
- `AuthConfig` in `crates/blufio-gateway/src/auth.rs` has `keypair_public_key: Option<VerifyingKey>` field
- `auth_middleware` checks: (1) fail-closed when no auth configured, (2) bearer token fast path, (3) keypair Ed25519 signature with `X-Signature`/`X-Timestamp` headers and 60-second replay prevention
- `GatewayChannelConfig` carries `keypair_public_key` through to `AuthConfig` via `GatewayChannel::connect()`
- `serve.rs` loads device keypair and passes its `verifying_key()` to gateway config
- Gateway refuses to start when enabled but no auth method is configured (fail-closed check in serve.rs)

### SC-4: Tool follow-up requests use the model selected by ModelRouter, not default_model
**Status:** PASS

**Evidence:**
- Tool follow-up `ProviderRequest` in `crates/blufio-agent/src/lib.rs` uses `actor.last_routing_decision()` to retrieve the session's routing decision
- When decision exists: uses `decision.actual_model` and `decision.max_tokens`
- When no decision: falls back to `self.config.anthropic.default_model` and `self.config.anthropic.max_tokens`
- Debug-level logging emitted for both paths showing model selection reasoning

## Build Verification

```
cargo check --workspace  -- PASS (clean, no warnings)
cargo test --workspace   -- PASS (586 tests, 0 failures)
```

## Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| LLM-05 | Satisfied | SC-1 (ToolUse/ToolResult blocks), SC-4 (routed model for follow-ups) |
| SEC-02 | Satisfied | SC-3 (keypair auth wired, fail-closed) |
| SEC-03 | Satisfied | SC-2 (vault_startup_check called before provider init) |

## Plans Completed

| Plan | Description | Status | Commit |
|------|-------------|--------|--------|
| 11-01 | P0: Tool content block serialization | Complete | a1bbc0c |
| 11-02 | P1: Vault startup wiring | Complete | b96ab36 |
| 11-03 | P2: Keypair auth gateway wiring | Complete | b0243b6 |
| 11-04 | P3: Model router bypass | Complete | a1bbc0c |

## Verdict

**PHASE COMPLETE** -- All 4 success criteria satisfied. All 3 requirements covered. All 4 plans executed successfully.
