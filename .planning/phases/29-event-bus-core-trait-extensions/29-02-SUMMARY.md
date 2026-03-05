---
phase: 29-event-bus-core-trait-extensions
plan: 02
status: complete
completed: "2026-03-05"
commit: e49022a
---

# Plan 02 Summary: Core Trait Extensions

## What was done

Extended blufio-core with provider-agnostic ToolDefinition, three media provider traits, and custom provider TOML configuration. Migrated Anthropic adapter, ToolRegistry, and agent loop to use typed ToolDefinition.

### Key changes

**blufio-core/src/types.rs:**
- Added `ToolDefinition` struct (name, description, input_schema) with `to_json_value()` method
- Changed `ProviderRequest.tools` from `Option<Vec<serde_json::Value>>` to `Option<Vec<ToolDefinition>>`
- Added `AdapterType::Tts`, `AdapterType::Transcription`, `AdapterType::ImageGen` (7 -> 10 variants)
- Added `TtsRequest`/`TtsResponse`, `TranscriptionRequest`/`TranscriptionResponse`, `ImageRequest`/`ImageResponse` types

**blufio-core/src/traits/:**
- Created `tts.rs` with `TtsAdapter` trait (synthesize, list_voices)
- Created `transcription.rs` with `TranscriptionAdapter` trait (transcribe)
- Created `image.rs` with `ImageAdapter` trait (generate)
- Updated `mod.rs` with 3 new module declarations and re-exports

**blufio-anthropic/src/lib.rs:**
- Replaced `serde_json::from_value()` conversion with direct field-by-field mapping from ToolDefinition to wire format

**blufio-skill/src/tool.rs:**
- Changed `ToolRegistry::tool_definitions()` return type from `Vec<serde_json::Value>` to `Vec<blufio_core::types::ToolDefinition>`

**blufio-config/src/model.rs:**
- Added `ProvidersConfig` and `CustomProviderConfig` structs
- Added `providers` field to `BlufioConfig`

**blufio-config/src/validation.rs:**
- Added validation for custom providers (wire_protocol, base_url scheme, api_key_env non-empty)

### Requirements satisfied

- **PROV-10**: Provider-agnostic ToolDefinition exists in blufio-core with name, description, input_schema
- **PROV-11**: ProviderRequest.tools uses `Vec<ToolDefinition>` (not `serde_json::Value`)
- **PROV-12**: TtsAdapter, TranscriptionAdapter, ImageAdapter traits defined in blufio-core
- **PROV-13**: Each provider can serialize ToolDefinition to its own wire format (Anthropic adapter demonstrated)
- **PROV-14**: Custom providers declared via TOML with base_url, wire_protocol, api_key_env

### Test results

Full workspace: all tests pass (0 failures)
Targeted crates: 197 tests pass (55 anthropic + 45 config + 11 core + 86 skill)
