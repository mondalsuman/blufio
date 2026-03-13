---
phase: 62-observability-api-surface
plan: 01
subsystem: infra
tags: [opentelemetry, utoipa, swagger-ui, insta, feature-flags, cargo-features, config]

# Dependency graph
requires: []
provides:
  - "Workspace deps: opentelemetry 0.31, opentelemetry_sdk, opentelemetry-otlp, tracing-opentelemetry, opentelemetry-http"
  - "Workspace deps: utoipa 5, utoipa-swagger-ui 9, insta 1"
  - "Feature flags: otel, swagger-ui, full on blufio binary crate"
  - "Feature flag: swagger-ui on blufio-gateway crate"
  - "Config types: OpenTelemetryConfig, ObservabilityConfig, LitestreamConfig, OpenApiConfig"
affects: [62-02, 62-03, 62-04, 62-05]

# Tech tracking
tech-stack:
  added: [opentelemetry 0.31, opentelemetry_sdk 0.31, opentelemetry-otlp 0.31, tracing-opentelemetry 0.32, opentelemetry-http 0.31, utoipa 5, utoipa-swagger-ui 9, insta 1]
  patterns: [feature-gated-optional-deps, observability-config-nesting, openapi-config-in-gateway]

key-files:
  created: []
  modified:
    - Cargo.toml
    - crates/blufio/Cargo.toml
    - crates/blufio-gateway/Cargo.toml
    - crates/blufio-config/src/model.rs

key-decisions:
  - "opentelemetry_sdk uses underscore (crate name), not hyphen (opentelemetry-sdk not found on crates.io)"
  - "utoipa is non-optional (always compiled) -- annotations are lightweight per user decision"
  - "Duplicate reqwest versions (0.12/0.13) are pre-existing, not introduced by OTel changes"
  - "OpenTelemetryConfig uses manual Default impl for non-trivial defaults (endpoint, ratios, sizes)"
  - "ObservabilityConfig wraps OpenTelemetryConfig in [observability.opentelemetry] TOML nesting"
  - "OpenApiConfig nested inside GatewayConfig as [gateway.openapi] section"

patterns-established:
  - "Feature-gated deps: otel feature enables 5 optional OTel crates, swagger-ui enables utoipa-swagger-ui"
  - "Config nesting: ObservabilityConfig wraps subsystem configs, OpenApiConfig nested in GatewayConfig"
  - "Full feature: convenience feature combining otel + swagger-ui for Docker builds"

requirements-completed: [OTEL-05, OTEL-06]

# Metrics
duration: 6min
completed: 2026-03-13
---

# Phase 62 Plan 01: Workspace Deps, Feature Flags, and Config Types Summary

**Workspace OpenTelemetry/utoipa/insta deps, otel/swagger-ui/full feature flags, and OpenTelemetryConfig/LitestreamConfig/OpenApiConfig config structs**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-13T10:55:55Z
- **Completed:** 2026-03-13T11:02:43Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Added 8 new workspace dependencies for OTel, OpenAPI, and snapshot testing
- Defined otel, swagger-ui, and full feature flags on blufio binary crate (NOT in default)
- Added 4 config structs (OpenTelemetryConfig, ObservabilityConfig, LitestreamConfig, OpenApiConfig) with serde defaults
- Verified zero OTel deps in default build (OTEL-05) and coexistence with Prometheus (OTEL-06)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add workspace dependencies and feature flags** - `c1f4fe1` (feat)
2. **Task 2: Add config types for all three subsystems** - `b42e09f` (feat)

## Files Created/Modified
- `Cargo.toml` - Added 8 workspace deps (opentelemetry family, utoipa, insta)
- `crates/blufio/Cargo.toml` - Added otel/swagger-ui/full features, optional OTel deps, non-optional utoipa, insta dev-dep
- `crates/blufio-gateway/Cargo.toml` - Added swagger-ui feature, utoipa (non-optional), utoipa-swagger-ui (optional), insta dev-dep
- `crates/blufio-config/src/model.rs` - Added ObservabilityConfig, OpenTelemetryConfig, LitestreamConfig, OpenApiConfig structs

## Decisions Made
- `opentelemetry_sdk` uses underscore in crate name (crates.io convention), not hyphen as in plan spec
- Pre-existing duplicate reqwest (0.12 + 0.13) in workspace is unaffected by OTel additions -- no resolution needed
- utoipa added as non-optional to both blufio and blufio-gateway (annotations always compiled, lightweight)
- OpenTelemetryConfig defaults: endpoint=localhost:4318, sample_ratio=1.0, batch_timeout_ms=5000, max_queue_size=2048

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed opentelemetry-sdk crate name to opentelemetry_sdk**
- **Found during:** Task 1 (cargo check)
- **Issue:** Plan specified `opentelemetry-sdk` but crates.io package name is `opentelemetry_sdk` (underscore)
- **Fix:** Changed to `opentelemetry_sdk` in workspace Cargo.toml, feature flag dep:references, and optional dependency
- **Files modified:** Cargo.toml, crates/blufio/Cargo.toml
- **Verification:** cargo check passes, cargo tree shows correct dependency
- **Committed in:** c1f4fe1 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Trivial naming fix. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Workspace dependencies are available for all downstream plans
- Feature flags otel, swagger-ui, full ready for #[cfg(feature = "otel")] guards
- Config types ready for OTel initialization (62-02), OpenAPI annotation (62-03), and Litestream CLI (62-04)
- insta dev-dependency available for OpenAPI snapshot testing

## Self-Check: PASSED

All files verified present. Both task commits (c1f4fe1, b42e09f) verified in git log.

---
*Phase: 62-observability-api-surface*
*Completed: 2026-03-13*
