# Plan 11-04 Summary: P3 Model Router Bypass in Tool Follow-up

**Phase:** 11-fix-integration-bugs
**Plan:** 04
**Status:** Complete
**Duration:** ~3 min (executed as part of combined P0+P3 commit)

## What Was Done

### Task 1: Use routed model for tool follow-up requests
- Replaced hardcoded `self.config.anthropic.default_model` in tool follow-up `ProviderRequest` construction with routing-decision-aware model selection
- Uses `actor.last_routing_decision()` to retrieve the `RoutingDecision` stored during initial request routing
- When routing decision exists: uses `decision.actual_model` and `decision.max_tokens`
- When no routing decision (routing disabled): falls back to `self.config.anthropic.default_model` and `self.config.anthropic.max_tokens`
- Debug-level logging emitted in both paths showing which model is used and why

## Files Modified

- `crates/blufio-agent/src/lib.rs` — routing-aware tool follow-up model selection

## Verification

- `cargo check --workspace` passes clean
- `cargo test --workspace` — 586 tests pass, 0 failures
- Follow-up `ProviderRequest` uses `decision.actual_model` when routing decision exists
- Falls back to `default_model` when no routing decision (no regression)
- Debug log confirms model selection path

## Commit

`a1bbc0c` — fix(P0+P3): tool content block serialization and model router follow-up
