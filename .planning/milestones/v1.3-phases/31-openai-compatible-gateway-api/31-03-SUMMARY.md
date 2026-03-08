---
phase: 31
plan: 03
status: complete
duration: ~10min
---

# Plan 31-03 Summary: Tools API (/v1/tools, /v1/tools/invoke)

## What was built

- **Tools wire types** (`tools_types.rs`): ToolListResponse, ToolInfo (with source, version, required_permissions metadata), ToolFunctionInfo, ToolInvokeRequest, ToolInvokeResponse, ToolsQueryParams.
- **Tool source detection**: `tool_source_from_name()` determines "builtin"/"mcp"/"wasm" from name pattern (__ separator = namespaced).
- **GET /v1/tools handler** (`tools.rs`): Returns tools filtered by config allowlist and optional ?source= query param, in OpenAI function schema format.
- **POST /v1/tools/invoke handler**: Executes tool directly (bypasses LLM) with allowlist enforcement. Returns result with duration_ms tracking.
- **Security**: Empty allowlist = no tools accessible (secure default). Non-allowed tools get 403 (permission_denied). Unknown tools get 404 (not_found).
- **GatewayConfig extended** (`blufio-config/src/model.rs`): Added `api_tools_allowlist: Vec<String>` field.
- **GatewayState extended**: Added `tools: Option<Arc<RwLock<ToolRegistry>>>` and `api_tools_allowlist`.
- **GatewayChannel extended** (`lib.rs`): Added `set_providers()`, `set_tools()`, `set_api_tools_allowlist()` methods.
- **Routes registered**: GET /v1/tools and POST /v1/tools/invoke behind auth middleware.

## Requirements covered

- API-09: GET /v1/tools with function schema format and metadata
- API-10: POST /v1/tools/invoke with config-based allowlist

## Test results

14 tests passing covering tool list serialization, invoke request/response serialization, source detection, query param deserialization, and tool metadata.
