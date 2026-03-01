---
phase: 07-wasm-skill-sandbox
plan: 01
status: complete
completed: 2026-03-01
commit: 77a023b
tests_added: 21 (blufio-skill) + 10 new (blufio-anthropic types/lib)
tests_total_pass: 82 across blufio-skill (21), blufio-anthropic (55), blufio-core (6)
---

# Plan 07-01 Summary: Tool Calling Foundation

## What Was Built

### New Crate: blufio-skill
- **Tool trait** (`tool.rs`): Unified async trait with `name()`, `description()`, `parameters_schema()`, `invoke()` methods
- **ToolOutput**: Struct with `content: String` and `is_error: bool`
- **ToolRegistry**: HashMap-backed registry with `register()`, `get()`, `list()`, `tool_definitions()` (Anthropic-format JSON)
- **BashTool** (`builtin/bash.rs`): Executes shell commands via `tokio::process::Command::new("bash").arg("-c")`, returns stdout/stderr, sets is_error on non-zero exit
- **HttpTool** (`builtin/http.rs`): Makes HTTP requests via reqwest, SSRF prevention via `blufio_security::ssrf::validate_url_host`, response truncation at 50KB
- **FileTool** (`builtin/file.rs`): Read/write files via `tokio::fs`, content truncation at 100KB for reads
- **register_builtins()**: Registers all 3 tools in one call

### Extended: blufio-core
- **BlufioError::Skill** variant: `{ message: String, source: Option<Box<dyn Error + Send + Sync>> }`
- **ToolUseData** struct: `{ id: String, name: String, input: serde_json::Value }`
- **ProviderStreamChunk**: Added `tool_use: Option<ToolUseData>` and `stop_reason: Option<String>` fields
- **ProviderRequest**: Added `tools: Option<Vec<serde_json::Value>>` field

### Extended: blufio-anthropic
- **ToolDefinition** struct: `{ name, description, input_schema }` for API serialization
- **ApiContentBlock**: Added `ToolUse` and `ToolResult` variants
- **ResponseContentBlock**: Added `ToolUse` variant for deserialization
- **MessageRequest**: Added `tools: Option<Vec<ToolDefinition>>` field (skip_serializing_if None)
- **Stateful stream mapping**: `map_stream_event_to_chunk_stateful()` replaces stateless version
  - Tracks active tool_use blocks by content block index
  - Accumulates `input_json_delta` partial JSON across deltas
  - Parses complete JSON on `content_block_stop`, emits `ProviderStreamChunk` with `ToolUseData`
  - Captures `stop_reason` from `message_delta` for downstream use
- **to_message_request()**: Converts `ProviderRequest.tools` (serde_json::Value) to `Vec<ToolDefinition>`

## Files Created
- `crates/blufio-skill/Cargo.toml`
- `crates/blufio-skill/src/lib.rs`
- `crates/blufio-skill/src/tool.rs`
- `crates/blufio-skill/src/builtin/mod.rs`
- `crates/blufio-skill/src/builtin/bash.rs`
- `crates/blufio-skill/src/builtin/http.rs`
- `crates/blufio-skill/src/builtin/file.rs`

## Files Modified
- `crates/blufio-core/src/error.rs` (added Skill variant)
- `crates/blufio-core/src/types.rs` (added ToolUseData, extended ProviderStreamChunk, ProviderRequest)
- `crates/blufio-anthropic/src/types.rs` (added ToolDefinition, ToolUse/ToolResult blocks, 8 new tests)
- `crates/blufio-anthropic/src/lib.rs` (stateful tool_use accumulation, tool mapping, 5 new tests)
- `crates/blufio-anthropic/src/client.rs` (tools field in test request)
- `crates/blufio-agent/src/context.rs` (tools: None)
- `crates/blufio-agent/src/heartbeat.rs` (tools: None)
- `crates/blufio-context/src/lib.rs` (tools: None)
- `crates/blufio-context/src/compaction.rs` (tools: None)
- `crates/blufio-memory/src/extractor.rs` (tools: None)

## Key Decisions
- Used stateful stream mapping in `lib.rs` rather than modifying the SSE parser (`sse.rs`) -- keeps the SSE layer as a pure event parser while the mapping layer handles tool_use accumulation
- SSRF prevention uses `blufio_security::ssrf::validate_url_host()` for static IP checks (no DNS resolution needed for the tool's URL validation)
- `ProviderRequest.tools` uses `serde_json::Value` for flexibility; conversion to `ToolDefinition` happens in `to_message_request()`
- Added `stop_reason` field to `ProviderStreamChunk` to allow downstream detection of `tool_use` stop reason

## Verification
- `cargo test -p blufio-skill`: 21/21 pass
- `cargo test -p blufio-anthropic`: 55/55 pass
- `cargo test -p blufio-core`: 6/6 pass
- `cargo check --workspace`: clean compilation
- Full workspace regression: all tests pass
