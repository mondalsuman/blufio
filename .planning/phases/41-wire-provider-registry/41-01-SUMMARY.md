---
phase: 41-wire-provider-registry
plan: 01
subsystem: api
tags: [provider-registry, openai, ollama, openrouter, gemini, feature-flags, gateway]

# Dependency graph
requires:
  - phase: 30-provider-crates
    provides: "OpenAI, Ollama, OpenRouter, Gemini provider adapter crates"
  - phase: 34-gateway
    provides: "ProviderRegistry trait in blufio-core"
provides:
  - "ConcreteProviderRegistry struct with ProviderRegistry trait impl"
  - "Feature-flagged provider dependencies in binary crate"
  - "Model routing (provider/model prefix splitting)"
  - "Static model lists for cloud providers"
  - "Dual constructors: from_config() for production, from_providers() for testing"
affects: [41-02, gateway, serve]

# Tech tracking
tech-stack:
  added: [blufio-openai, blufio-ollama, blufio-openrouter, blufio-gemini]
  patterns: [feature-gated-provider-init, config-required-activation, dual-constructor-testing]

key-files:
  created:
    - crates/blufio/src/providers.rs
  modified:
    - crates/blufio/Cargo.toml
    - crates/blufio/src/main.rs

key-decisions:
  - "Ollama provider stored as separate Arc<OllamaProvider> field (avoids Any downcast for list_local_models)"
  - "async-trait added as regular dependency (needed for ProviderRegistry trait impl)"
  - "All four provider features added to default feature set"

patterns-established:
  - "Config-required activation: providers only init if their config has meaningful values (api_key or default_model)"
  - "Graceful degradation: non-default provider init failures log warn and skip; default provider failure is hard error"

requirements-completed: [PROV-01, PROV-02, PROV-03, PROV-04, PROV-05, PROV-06, PROV-07, PROV-08, PROV-09]

# Metrics
duration: 7min
completed: 2026-03-07
---

# Phase 41 Plan 01: Provider Registry Summary

**ConcreteProviderRegistry with feature-gated provider init, config-required activation, model routing, and static model lists for all five providers**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-07T20:54:40Z
- **Completed:** 2026-03-07T21:02:18Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added blufio-openai, blufio-ollama, blufio-openrouter, blufio-gemini as optional dependencies with feature flags
- Implemented ConcreteProviderRegistry with from_config() (feature-gated, config-required) and from_providers() (testing)
- Model routing splits "provider/model" on first "/" and routes unprefixed models to default provider
- Static model lists for OpenAI (6), Anthropic (3), Gemini (3); OpenRouter empty (pass-through); Ollama dynamic via list_local_models()
- 11 unit tests covering constructors, routing, model listing, and filtering

## Task Commits

Each task was committed atomically:

1. **Task 1: Add provider crate dependencies and feature flags** - `1df294a` (feat)
2. **Task 2: Implement ConcreteProviderRegistry** - `f64c08a` (feat)

## Files Created/Modified
- `crates/blufio/Cargo.toml` - Added 4 optional provider deps, 4 feature flags, async-trait dep
- `crates/blufio/src/providers.rs` - ConcreteProviderRegistry with ProviderRegistry trait impl
- `crates/blufio/src/main.rs` - Added `mod providers` declaration

## Decisions Made
- Stored Ollama provider as separate `Arc<OllamaProvider>` field behind `#[cfg(feature = "ollama")]` to avoid `Any` downcast for `list_local_models()` access
- Added async-trait as regular dependency (was only in dev-dependencies) since `#[async_trait] impl ProviderRegistry` needs it at compile time
- All four new provider features included in the `default` feature list, matching the pattern for anthropic and other adapters

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed lifetime mismatch in resolve_model()**
- **Found during:** Task 2 (ConcreteProviderRegistry implementation)
- **Issue:** `resolve_model(&self, model: &str) -> (&str, &str)` had conflicting lifetimes -- returns borrowed from both `self` and `model` parameter
- **Fix:** Added explicit lifetime parameter `<'a>` binding both input and output
- **Files modified:** crates/blufio/src/providers.rs
- **Verification:** cargo test passes with all features
- **Committed in:** f64c08a (Task 2 commit)

**2. [Rule 3 - Blocking] Replaced downcast approach with separate Ollama field**
- **Found during:** Task 2 (list_models implementation)
- **Issue:** `dyn ProviderAdapter + Send + Sync` does not implement `Any`, so downcast_ref to OllamaProvider impossible
- **Fix:** Added `ollama: Option<Arc<OllamaProvider>>` field to struct behind `#[cfg(feature = "ollama")]`, stored during from_config() init
- **Files modified:** crates/blufio/src/providers.rs
- **Verification:** Compiles cleanly, list_models works for all providers
- **Committed in:** f64c08a (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both fixes were necessary for correct compilation. No scope creep.

## Issues Encountered
- Dead code warnings for ConcreteProviderRegistry in non-test builds -- expected since serve.rs wiring happens in Plan 02

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- ConcreteProviderRegistry ready to be wired into serve.rs and GatewayState (Plan 02)
- All provider feature flags compile and test successfully
- 11 unit tests provide coverage for registry operations

## Self-Check: PASSED

- FOUND: crates/blufio/src/providers.rs (488 lines, min_lines: 80)
- FOUND: crates/blufio/Cargo.toml (modified)
- FOUND: crates/blufio/src/main.rs (modified)
- FOUND: commit 1df294a (Task 1)
- FOUND: commit f64c08a (Task 2)
- FOUND: 41-01-SUMMARY.md

---
*Phase: 41-wire-provider-registry*
*Completed: 2026-03-07*
