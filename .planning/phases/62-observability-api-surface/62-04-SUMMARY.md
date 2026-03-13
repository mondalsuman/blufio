---
phase: 62-observability-api-surface
plan: 04
subsystem: api
tags: [openapi, utoipa, swagger-ui, rest-api, documentation]

# Dependency graph
requires:
  - phase: 62-01
    provides: "utoipa workspace deps, swagger-ui feature flag, config types"
provides:
  - "OpenAPI 3.1 spec auto-generated from utoipa annotations on all 18 gateway handlers"
  - "/openapi.json public endpoint serving machine-readable API spec"
  - "Swagger UI at /docs when swagger-ui feature compiled and config enabled"
  - "Snapshot test for OpenAPI spec regression detection"
affects: [63-deployment]

# Tech tracking
tech-stack:
  added: [utoipa v5 annotations, utoipa-swagger-ui v9, insta snapshot testing]
  patterns: [utoipa::path annotations on handlers, ToSchema derives on wire types, SecurityAddon modifier for Bearer auth, feature-gated Swagger UI]

key-files:
  created:
    - "crates/blufio-gateway/src/openapi.rs"
    - "crates/blufio-gateway/src/snapshots/blufio_gateway__openapi__tests__openapi_spec.snap"
  modified:
    - "crates/blufio-gateway/src/handlers.rs"
    - "crates/blufio-gateway/src/openai_compat/handlers.rs"
    - "crates/blufio-gateway/src/openai_compat/responses.rs"
    - "crates/blufio-gateway/src/openai_compat/responses_types.rs"
    - "crates/blufio-gateway/src/openai_compat/tools.rs"
    - "crates/blufio-gateway/src/openai_compat/tools_types.rs"
    - "crates/blufio-gateway/src/openai_compat/types.rs"
    - "crates/blufio-gateway/src/api_keys/mod.rs"
    - "crates/blufio-gateway/src/api_keys/handlers.rs"
    - "crates/blufio-gateway/src/webhooks/mod.rs"
    - "crates/blufio-gateway/src/webhooks/handlers.rs"
    - "crates/blufio-gateway/src/batch/mod.rs"
    - "crates/blufio-gateway/src/batch/handlers.rs"
    - "crates/blufio-gateway/src/lib.rs"
    - "crates/blufio-gateway/src/server.rs"

key-decisions:
  - "Module named openapi.rs (not openapi_doc.rs) -- no utoipa::openapi namespace conflict at crate level"
  - "ModelsListResponse.data uses #[schema(value_type = Vec<Object>)] since ModelInfo is from blufio-core (no ToSchema)"
  - "swagger_ui_enabled added to ServerConfig (config-driven toggle, not just feature gate)"
  - "/openapi.json is public (no auth) to support CI tooling, Postman imports, and code generators"

patterns-established:
  - "utoipa::path annotation pattern: tag, request_body, responses, security for all handlers"
  - "ToSchema derive on all request/response wire types in gateway"
  - "IntoParams derive on query parameter structs (ModelsQueryParams, ToolsQueryParams)"
  - "SecurityAddon modifier pattern for Bearer auth in OpenAPI spec"

requirements-completed: [OAPI-01, OAPI-02, OAPI-03, OAPI-04]

# Metrics
duration: 14min
completed: 2026-03-13
---

# Phase 62 Plan 04: OpenAPI Spec Summary

**OpenAPI 3.1 spec via utoipa on all 18 gateway handlers, /openapi.json endpoint, Swagger UI at /docs, insta snapshot test**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-13T11:05:26Z
- **Completed:** 2026-03-13T11:19:59Z
- **Tasks:** 2
- **Files modified:** 17

## Accomplishments
- All 18 gateway handler functions annotated with #[utoipa::path] including correct HTTP methods, paths, tags, request bodies, responses with body types, path params, query params, and security requirements
- 30+ request/response types annotated with ToSchema derives with schema examples on key fields
- ApiDoc struct aggregates all paths and schemas with 7 tags (Messages, Sessions, OpenAI Compatible, API Keys, Webhooks, Batch, Health) and Bearer auth security scheme
- /openapi.json public route serves auto-generated OpenAPI 3.1 JSON spec
- Swagger UI at /docs feature-gated behind swagger-ui Cargo feature and config-driven toggle
- Snapshot test captures full spec for regression detection

## Task Commits

Each task was committed atomically:

1. **Task 1: Add utoipa annotations to all handler types and functions** - `51eb069` (feat)
2. **Task 2: Create ApiDoc struct, /openapi.json route, Swagger UI, and snapshot test** - `ab426c8` (feat)

## Files Created/Modified
- `crates/blufio-gateway/src/openapi.rs` - ApiDoc struct with all handler paths, component schemas, tags, security
- `crates/blufio-gateway/src/snapshots/blufio_gateway__openapi__tests__openapi_spec.snap` - Insta snapshot of full OpenAPI spec
- `crates/blufio-gateway/src/handlers.rs` - ToSchema derives + #[utoipa::path] on 5 core handlers
- `crates/blufio-gateway/src/openai_compat/handlers.rs` - #[utoipa::path] on chat completions and models
- `crates/blufio-gateway/src/openai_compat/responses.rs` - #[utoipa::path] on POST /v1/responses
- `crates/blufio-gateway/src/openai_compat/responses_types.rs` - ToSchema on responses wire types
- `crates/blufio-gateway/src/openai_compat/tools.rs` - #[utoipa::path] on tools list and invoke
- `crates/blufio-gateway/src/openai_compat/tools_types.rs` - ToSchema on tools wire types
- `crates/blufio-gateway/src/openai_compat/types.rs` - ToSchema on OpenAI compat wire types
- `crates/blufio-gateway/src/api_keys/mod.rs` - ToSchema on API key types
- `crates/blufio-gateway/src/api_keys/handlers.rs` - #[utoipa::path] on API key CRUD handlers
- `crates/blufio-gateway/src/webhooks/mod.rs` - ToSchema on webhook types
- `crates/blufio-gateway/src/webhooks/handlers.rs` - #[utoipa::path] on webhook CRUD handlers
- `crates/blufio-gateway/src/batch/mod.rs` - ToSchema on batch types
- `crates/blufio-gateway/src/batch/handlers.rs` - #[utoipa::path] on batch submit and status handlers
- `crates/blufio-gateway/src/lib.rs` - Added pub mod openapi
- `crates/blufio-gateway/src/server.rs` - /openapi.json route, Swagger UI merge, swagger_ui_enabled config

## Decisions Made
- Module named `openapi.rs` (not `openapi_doc.rs`): no actual namespace conflict since crate::openapi resolves to our module
- ModelsListResponse.data uses `#[schema(value_type = Vec<Object>)]` because ModelInfo lives in blufio-core without ToSchema derive
- swagger_ui_enabled added to ServerConfig as config-driven toggle, not purely feature-gate controlled
- /openapi.json is public (no auth required) to support CI tooling, Postman imports, and code generators

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- Module naming: initially tried `openapi_doc.rs` to avoid potential conflict with `utoipa::openapi` submodule, but linter kept reverting; reverted to `openapi.rs` which works correctly since Rust resolves `crate::openapi` to the local module
- cargo-insta CLI not installed: accepted snapshot manually by renaming `.snap.new` to `.snap`

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- OpenAPI spec generation complete, all handlers documented
- /openapi.json endpoint ready for external consumers and CI integration
- Swagger UI available when swagger-ui feature is compiled and enabled in config

---
*Phase: 62-observability-api-surface*
*Completed: 2026-03-13*
