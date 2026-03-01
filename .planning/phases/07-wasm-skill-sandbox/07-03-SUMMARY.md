# Plan 07-03 Summary: Agent Integration (Wave 2)

## Status: COMPLETE

## What was built

### Task 1: SkillProvider implementing ConditionalProvider
- **`crates/blufio-skill/src/provider.rs`**: `SkillProvider` struct with `Arc<RwLock<ToolRegistry>>` and `max_skills_in_prompt`. Implements `ConditionalProvider` to inject tool one-liners ("## Available Tools\nname: description") into the conditional zone of the LLM prompt. Returns empty vec when no tools are registered. Truncates at max_skills_in_prompt and appends "... and N more tools available".
- **`crates/blufio-skill/Cargo.toml`**: Added `blufio-context` dependency
- **`crates/blufio-skill/src/lib.rs`**: Added `pub mod provider;` and `pub use provider::SkillProvider;`
- 4 provider tests

### Task 2: SessionActor with tool_use/tool_result loop
- **`crates/blufio-agent/src/session.rs`**:
  - Added `ToolExecuting` variant to `SessionState` enum
  - Added `tool_registry: Arc<RwLock<ToolRegistry>>` and `max_tool_iterations: usize` fields
  - Added `MAX_TOOL_ITERATIONS` constant (10)
  - Added `execute_tools()` method: looks up tools in registry, invokes them, returns `(tool_use_id, ToolOutput)` pairs. Handles missing tools gracefully with error output.
  - Tool definitions from ToolRegistry injected into ProviderRequest before streaming
  - Updated constructor to accept `tool_registry` parameter
  - New getters: `max_tool_iterations()`, `tool_registry()`
- **`crates/blufio-agent/src/lib.rs`**:
  - Added `tool_registry: Arc<RwLock<ToolRegistry>>` field to `AgentLoop`
  - Updated constructor to accept `tool_registry` parameter
  - Refactored `handle_inbound` with tool_use/tool_result loop:
    1. Consumes stream via `consume_stream()` helper
    2. Detects tool_use blocks and stop_reason "tool_use"
    3. Executes tools via `session.execute_tools()`
    4. Persists assistant (with tool_use) and user (with tool_result) messages
    5. Re-calls LLM with tool results
    6. Loops up to `max_tool_iterations` (10)
  - Added `consume_stream()` standalone helper function
  - Updated `resolve_or_create_session` to pass tool_registry to SessionActor
- **`crates/blufio-agent/Cargo.toml`**: Added `blufio-skill` dependency
- 30 agent tests pass (4 new: state display, state equality, max iterations constant, tool registry sharing)

### Task 3: CLI subcommand and wiring
- **`crates/blufio/src/main.rs`**:
  - Added `Skill` variant to `Commands` enum with `SkillCommands` subcommand
  - `SkillCommands` enum: `Init { name }`, `List`, `Install { wasm_path, manifest_path }`, `Remove { name }`
  - `handle_skill_command()`: init calls `scaffold_skill()`, list queries `SkillStore`, install reads manifest + registers in store, remove calls `store.remove()`
  - 4 CLI parsing tests
- **`crates/blufio/src/serve.rs`**:
  - Creates `ToolRegistry` with built-in tools on startup
  - Creates `SkillProvider` and registers it with `ContextEngine` as conditional provider
  - Passes `tool_registry` to `AgentLoop::new`
- **`crates/blufio/Cargo.toml`**: Added `blufio-skill` dependency

## Key decisions
- **Tool loop in agent loop, not session**: The tool_use/tool_result loop lives in `handle_inbound` (lib.rs), not deep in session.rs. The session provides `execute_tools()` and the loop orchestration stays in the agent loop where it can manage stream consumption and channel communication.
- **consume_stream helper**: Extracted stream consumption into a standalone function to reuse across initial stream and follow-up tool loop iterations.
- **SkillProvider before Arc**: The SkillProvider is registered with ContextEngine before wrapping in Arc, keeping the flow simple.
- **Tool definitions always injected**: When the tool registry is non-empty, tool definitions are always included in the ProviderRequest, enabling the LLM to use tools on every turn.

## Verification
- `cargo test --workspace`: all tests pass (0 failures across all crates)
- `cargo check --workspace`: clean compilation
- blufio-skill: 57 tests (53 existing + 4 new provider tests)
- blufio-agent: 30 tests (26 existing + 4 new)
- blufio (binary): 12 tests (8 existing + 4 new skill CLI parsing tests)

## Files modified/created
- `crates/blufio-skill/Cargo.toml`: added blufio-context dependency
- `crates/blufio-skill/src/lib.rs`: added provider module and re-export
- `crates/blufio-skill/src/provider.rs` (new): SkillProvider implementing ConditionalProvider
- `crates/blufio-agent/Cargo.toml`: added blufio-skill dependency
- `crates/blufio-agent/src/session.rs`: ToolExecuting state, tool_registry field, execute_tools method
- `crates/blufio-agent/src/lib.rs`: tool_registry field, tool_use/tool_result loop, consume_stream helper
- `crates/blufio/Cargo.toml`: added blufio-skill dependency
- `crates/blufio/src/main.rs`: Skill subcommand (init, list, install, remove), handler, CLI tests
- `crates/blufio/src/serve.rs`: ToolRegistry creation, SkillProvider wiring, tool_registry passing
