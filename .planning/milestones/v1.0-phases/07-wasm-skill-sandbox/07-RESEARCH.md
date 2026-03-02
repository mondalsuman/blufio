# Phase 7: WASM Skill Sandbox - Research

**Researched:** 2026-03-01
**Domain:** WASM sandboxing, capability-controlled tool execution, skill registry
**Confidence:** HIGH

## Summary

Phase 7 implements the skill and tool execution layer: built-in tools (bash, HTTP, file I/O) registered through a unified ToolRegistry, and third-party WASM skills running in wasmtime sandboxes with fuel metering, memory limits, and epoch interruption. The Anthropic tool_use/tool_result flow must be wired into the provider and session FSM to enable the LLM to invoke tools.

The stack is mature: wasmtime 40.x provides production-grade WASM sandboxing with well-documented fuel, memory, and epoch controls. The WASI preview2 APIs offer granular filesystem and network capability gating via WasiCtxBuilder. The WIT Component Model is the recommended approach for host-guest contracts in wasmtime 40.x.

**Primary recommendation:** Use wasmtime 40.x with WIT Component Model for WASM skills, raw import/export host functions for the minimal host API (log, http_request, read_file, write_file, get_env). Wire tool_use blocks into ProviderStreamChunk and extend the session FSM with a ToolExecution state.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Unified registry: built-in tools and WASM skills share the same tool_use interface. Built-ins run natively instead of in WASM but are registered identically
- Full bash access with no restrictions — this is a personal agent on a single-user VPS
- HTTP requests use existing SSRF prevention (private IP blocking) and TLS enforcement from blufio-security — no additional guards
- Full filesystem access for the file I/O tool, consistent with bash access
- Tool calling needs wiring into the Anthropic provider (tool_use/tool_result flow doesn't exist yet)
- Minimal host functions exposed to WASM skills: log(level, msg), http_request(url, method, headers, body), read_file(path), write_file(path, data), get_env(key)
- Each host function gated by capability manifest — if the skill doesn't declare network permission, http_request is unavailable
- Rust-only SDK at launch; `blufio skill init` scaffolds a Rust project
- Other languages that compile to WASM can work but without official templates
- Install-time approval: when installing a skill, its capabilities are displayed and the user approves once (like Android app permissions)
- Path-scoped filesystem permissions: skills declare specific directories they need (e.g., read: ["~/.config/myapp"], write: ["/tmp/skill-output"])
- Domain-scoped network permissions: skills declare which domains they need access to (e.g., ["api.github.com", "*.openai.com"])
- Conservative resource defaults: 16MB memory max, 1 billion fuel instructions (~1-2 seconds CPU), 5 second epoch timeout
- Resource limits configurable per-skill in manifest
- Name + one-liner format in the agent's prompt: `skill_name: Brief description of what it does`
- Full SKILL.md documentation loaded only when the LLM decides to invoke a skill
- SKILL.md is usage-focused and written for the LLM: purpose, parameters with types, example invocations, expected output format
- SkillProvider implements ConditionalProvider to inject skill one-liners into the conditional context zone
- On tool invocation, full SKILL.md loaded into the dynamic zone

### Claude's Discretion
- WASM host-guest contract approach (WIT Component Model vs raw import/export) — pick based on wasmtime best practices
- Skill I/O format (JSON vs structured types) — tied to contract approach choice
- Skill cap in prompt — size based on token overhead analysis
- Loading skeleton and error state design for skill execution feedback
- Exact SKILL.md template structure
- Sandbox epoch interrupt implementation details

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SEC-05 | WASM skill sandbox (wasmtime) with capability manifests -- skills cannot escape sandbox | wasmtime 40.x sandbox isolation, WasiCtxBuilder preopened_dir and network controls, fuel + epoch for resource limits |
| SEC-06 | WASM sandbox enforces fuel limits (CPU), memory limits, and epoch interruption | Config::consume_fuel, Store::set_fuel, Config::epoch_interruption, Engine::increment_epoch, ResourceLimiter trait |
| SKILL-01 | Built-in tools: bash execution, HTTP requests, file I/O with capability controls | ToolRegistry trait with BuiltinTool implementations, Anthropic tool_use/tool_result flow |
| SKILL-02 | WASM skill sandbox executes third-party skills in isolated wasmtime instances | WasmSkillRuntime using per-invocation Store with fuel/memory/epoch limits |
| SKILL-03 | Skill capability manifests declare required permissions (network, filesystem paths, etc.) | TOML manifest with [capabilities] section, validated at install-time |
| SKILL-04 | Progressive skill discovery: agent sees skill names + descriptions in prompt, loads full SKILL.md on demand | SkillProvider as ConditionalProvider, dynamic SKILL.md loading on tool invocation |
| SKILL-05 | Skill registry tracks installed skills with version, capabilities, and verification status | SQLite-backed SkillRegistry with installed_skills table |
| SKILL-06 | `blufio skill init` creates working skill scaffold in 3 commands | CLI subcommand with template generation, Cargo.toml + lib.rs + skill.toml |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| wasmtime | 40.x | WASM runtime with sandbox isolation | Bytecode Alliance reference implementation, production-grade, best Rust integration |
| wasmtime-wasi | 40.x | WASI preview2 filesystem/network controls | Granular preopened_dir, socket_addr_check, allow_tcp/udp controls |
| wit-bindgen | 0.38+ | WIT Component Model guest bindings | Official bindings generator for Component Model |
| toml | 0.8 (workspace) | Skill manifest parsing | Already in workspace, familiar format for Rust developers |
| serde | 1 (workspace) | Manifest/invocation serialization | Already in workspace |
| serde_json | 1 | Skill I/O format (JSON over Component Model boundary) | Simple, debuggable, LLM-native format |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio (workspace) | 1 | Async runtime for epoch ticker thread | Already in workspace, needed for background epoch increment |
| reqwest (workspace) | 0.12 | HTTP host function implementation | Already in workspace for HTTP tool |
| rusqlite (workspace) | 0.37 | Skill registry persistence | Already in workspace, same SQLite pattern |
| glob | 0.3 | Skill discovery in skills directory | Finding installed skill directories |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| WIT Component Model | Raw WASM imports/exports | Simpler but less type-safe, no automatic bindgen; WIT is wasmtime's recommended path |
| JSON I/O | WIT structured types | WIT types are more type-safe but harder for LLMs to format; JSON is LLM-native |
| TOML manifest | YAML/JSON manifest | TOML is consistent with existing Blufio config (blufio.toml), familiar to Rust devs |

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio-skill/         # New crate: WASM sandbox, skill registry, built-in tools
│   ├── src/
│   │   ├── lib.rs
│   │   ├── registry.rs     # ToolRegistry + SkillRegistry
│   │   ├── builtin/        # Built-in tool implementations
│   │   │   ├── mod.rs
│   │   │   ├── bash.rs     # BashTool
│   │   │   ├── http.rs     # HttpTool
│   │   │   └── file.rs     # FileTool
│   │   ├── sandbox.rs      # WasmSkillRuntime (wasmtime sandbox)
│   │   ├── manifest.rs     # SkillManifest parsing/validation
│   │   ├── provider.rs     # SkillProvider (ConditionalProvider impl)
│   │   ├── scaffold.rs     # `blufio skill init` template generation
│   │   └── store.rs        # SQLite skill registry persistence
│   └── Cargo.toml
```

### Pattern 1: Unified Tool Interface
**What:** Both built-in tools and WASM skills implement the same trait and register in the same ToolRegistry. The LLM sees them identically via tool_use.
**When to use:** Always — the user locked this decision.
**Example:**
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value; // JSON Schema for tool_use
    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}
```

### Pattern 2: Per-Invocation Sandbox
**What:** Each WASM skill invocation creates a fresh Store with its own fuel/memory limits. No state persists between invocations (stateless by default).
**When to use:** Every WASM skill execution.
**Example:**
```rust
// Source: wasmtime docs - Config::consume_fuel, Store::set_fuel
let mut config = Config::new();
config.consume_fuel(true);
config.epoch_interruption(true);

let engine = Engine::new(&config)?;
let mut store = Store::new(&engine, SkillState::new(manifest.clone()));
store.set_fuel(manifest.fuel_limit.unwrap_or(1_000_000_000))?;
store.epoch_deadline_trap(manifest.epoch_timeout.unwrap_or(5))?;

// Background epoch ticker
let engine_clone = engine.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        engine_clone.increment_epoch();
    }
});
```

### Pattern 3: Capability-Gated Host Functions
**What:** Host functions check the skill's manifest before executing. If a capability isn't declared, the function traps with a permission error.
**When to use:** Every host function exposed to WASM skills.
**Example:**
```rust
// Define host function that checks capabilities
linker.func_wrap("env", "http_request", |caller: Caller<'_, SkillState>, url_ptr: i32, ...| {
    let state = caller.data();
    if !state.manifest.capabilities.network.is_some() {
        return Err(anyhow!("skill does not have network permission"));
    }
    let allowed_domains = &state.manifest.capabilities.network.as_ref().unwrap().domains;
    // Check URL domain against allowed_domains
    // Execute request if permitted
})?;
```

### Pattern 4: Tool Calling in Session FSM
**What:** Extend the session FSM with a ToolExecution state. When the LLM returns stop_reason="tool_use", parse tool calls, execute them, return tool_result, and re-call the LLM.
**When to use:** Every message that triggers tool invocation.
**Example:**
```
Idle -> Receiving -> Processing -> [LLM call] ->
  if stop_reason == "tool_use":
    ToolExecuting -> [execute tools] -> Processing -> [LLM call with tool_result] ->
  Responding -> Idle
```

### Anti-Patterns to Avoid
- **Shared Store across invocations:** Creates state leakage and security risk. Always create a fresh Store per invocation.
- **Unbounded epoch ticker:** The epoch ticker thread must be scoped to the invocation lifetime, not the application lifetime. Use a JoinHandle and abort on completion.
- **Trusting WASM skill output:** Always sanitize and validate skill output before returning to the LLM. Malicious skills could try prompt injection.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WASM sandbox isolation | Custom process isolation | wasmtime Engine/Store | Battle-tested sandbox with memory/fuel/epoch controls |
| Filesystem capability control | Manual path checking | WasiCtxBuilder::preopened_dir | Kernel-level path containment, prevents `..` traversal |
| Network capability control | Manual domain filtering in reqwest | Host function with domain allowlist | Simpler, checked at function call boundary |
| JSON Schema for tool_use | Manual schema construction | serde_json + schemars crate | Auto-derives JSON Schema from Rust types |
| Template generation | String concatenation | include_str! + template strings | Maintainable, testable scaffold templates |

**Key insight:** wasmtime provides all the sandboxing primitives. The real engineering is in the integration: wiring tool_use/tool_result into the session FSM, bridging async host functions with the WASM sandbox, and making the capability manifest ergonomic.

## Common Pitfalls

### Pitfall 1: Blocking in Async Host Functions
**What goes wrong:** wasmtime host functions run synchronously by default. Calling async operations (reqwest, tokio-rusqlite) blocks the executor.
**Why it happens:** wasmtime's Linker::func_wrap expects synchronous closures.
**How to avoid:** Use wasmtime's async support: Config::async_support(true) and Linker::func_wrap_async. Or use tokio::task::block_in_place for simple cases.
**Warning signs:** Executor starvation, high latency on tool calls.

### Pitfall 2: Engine/Module Reuse vs Store Isolation
**What goes wrong:** Developers create a new Engine per invocation, paying compilation cost each time.
**Why it happens:** Confusion between Engine (compilation) and Store (execution state).
**How to avoid:** Create Engine once at startup, compile Module/Component once, create fresh Store per invocation. Engine is thread-safe and cheap to clone (Arc internally).
**Warning signs:** Slow first invocation, high CPU on skill calls.

### Pitfall 3: Fuel Calibration
**What goes wrong:** Fuel limits either too low (legitimate skills trap prematurely) or too high (malicious skills consume excessive CPU).
**Why it happens:** Fuel units don't map linearly to wall-clock time; they depend on instruction mix.
**How to avoid:** Default 1 billion fuel (~1-2s CPU). Combine with epoch interruption (5s wall-clock deadline) as a hard backstop. Let manifest override defaults.
**Warning signs:** Skills trapping mid-computation, users raising fuel limits blindly.

### Pitfall 4: Tool Calling Loop Depth
**What goes wrong:** LLM enters infinite tool-calling loop (calls tool A, which returns data causing it to call tool A again).
**Why it happens:** No recursion limit on tool_use iterations.
**How to avoid:** Cap tool_use iterations per turn (e.g., max 10 rounds). If exceeded, force a text response.
**Warning signs:** Token costs spike, response latency increases dramatically.

### Pitfall 5: Anthropic tool_use Response Parsing
**What goes wrong:** Tool use blocks in streaming responses require accumulating partial JSON across content_block_delta events (type: "input_json_delta").
**Why it happens:** Tool input is streamed as partial JSON chunks, not delivered atomically.
**How to avoid:** Accumulate partial_json strings per content block index. Parse complete JSON only on content_block_stop.
**Warning signs:** JSON parse errors, incomplete tool inputs.

## Code Examples

### Anthropic Tool Definition Format
```json
{
  "name": "bash",
  "description": "Execute a bash command and return stdout/stderr",
  "input_schema": {
    "type": "object",
    "properties": {
      "command": { "type": "string", "description": "The bash command to execute" }
    },
    "required": ["command"]
  }
}
```

### Anthropic tool_use Response Block
```json
{
  "type": "tool_use",
  "id": "toolu_01A09q90qw90lq917835lq9",
  "name": "bash",
  "input": { "command": "ls -la" }
}
```

### Anthropic tool_result Message
```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01A09q90qw90lq917835lq9",
      "content": "total 48\ndrwxr-xr-x 12 user user 384 Mar  1 10:00 .\n..."
    }
  ]
}
```

### Skill Manifest (skill.toml)
```toml
[skill]
name = "github-pr"
version = "0.1.0"
description = "Create and manage GitHub pull requests"
author = "skill-author"

[capabilities]
network = { domains = ["api.github.com"] }
filesystem = { read = [], write = ["/tmp/gh-cache"] }
env = ["GITHUB_TOKEN"]

[resources]
fuel = 2_000_000_000    # 2B instructions (~3-4s CPU)
memory_mb = 32           # 32MB
epoch_timeout_secs = 10  # 10s wall-clock

[wasm]
entry = "skill.wasm"
```

### wasmtime Sandbox Setup
```rust
// Source: wasmtime docs - Engine, Store, Config
use wasmtime::*;

fn create_sandbox(manifest: &SkillManifest) -> Result<(Engine, Store<SkillState>)> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.async_support(true); // For async host functions

    let engine = Engine::new(&config)?;
    let mut store = Store::new(&engine, SkillState::new(manifest.clone()));

    // Set fuel limit
    let fuel = manifest.resources.fuel.unwrap_or(1_000_000_000);
    store.set_fuel(fuel)?;

    // Set epoch deadline (in epochs from now)
    let epochs = manifest.resources.epoch_timeout_secs.unwrap_or(5);
    store.epoch_deadline_trap(epochs as u64)?;

    Ok((engine, store))
}
```

### WasiCtxBuilder for Capability Gating
```rust
// Source: wasmtime-wasi docs - WasiCtxBuilder
use wasmtime_wasi::{WasiCtxBuilder, DirPerms, FilePerms};

fn build_wasi_ctx(manifest: &SkillManifest) -> WasiCtx {
    let mut builder = WasiCtxBuilder::new();

    // Only grant filesystem access if declared
    if let Some(ref fs) = manifest.capabilities.filesystem {
        for read_path in &fs.read {
            builder.preopened_dir(read_path, read_path, DirPerms::READ, FilePerms::READ)
                .expect("failed to preopen read dir");
        }
        for write_path in &fs.write {
            builder.preopened_dir(write_path, write_path, DirPerms::all(), FilePerms::all())
                .expect("failed to preopen write dir");
        }
    }

    // Only grant network if declared
    if manifest.capabilities.network.is_some() {
        // Network is handled via custom host functions, not WASI networking
        // Domain filtering happens in the http_request host function
    }

    builder.build()
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| WASI preview1 (wasi_snapshot_preview1) | WASI preview2 (Component Model) | wasmtime 18+ (2024) | Better capability isolation, typed interfaces |
| Raw WASM imports/exports | WIT Component Model | wasmtime 14+ (2023) | Type-safe host-guest contracts, auto-generated bindings |
| Manual fuel tracking | Config::consume_fuel + Store::set_fuel | Stable since wasmtime 1.0 | Built-in fuel metering, no custom instrumentation |
| Process-based isolation | In-process wasmtime sandbox | Always available | Lower overhead, same security guarantees for WASM |

**Deprecated/outdated:**
- wasmtime 0.x API: Completely restructured in 1.0+. All Context7 examples use current API.
- `wasmtime::Linker::module` for core modules: Still works but Component Model (`Linker::instantiate`) is the recommended path for new code.

## Open Questions

1. **WIT vs raw imports for host functions**
   - What we know: WIT Component Model is wasmtime's recommended path. Raw imports are simpler for the 5 host functions we expose.
   - What's unclear: Whether the complexity of WIT setup is justified for only 5 functions.
   - Recommendation: Use raw WASM imports (not WIT) for the host functions. The skill guest uses wit-bindgen only for its own export interface. The host defines imports via Linker::func_wrap. This avoids WIT complexity for the host side while keeping the guest SDK ergonomic.

2. **Async host functions approach**
   - What we know: http_request and read_file/write_file are inherently async. wasmtime supports async via Config::async_support.
   - What's unclear: Whether async support adds significant overhead vs block_in_place.
   - Recommendation: Use Config::async_support(true) since the agent is already fully async (tokio). This avoids blocking the executor.

3. **Skill cap in prompt**
   - What we know: Each skill one-liner is ~50-100 tokens. At 20 skills, that's 1,000-2,000 tokens.
   - What's unclear: The optimal cap before context becomes too noisy.
   - Recommendation: Cap at 20 skills in the prompt. Beyond that, the LLM sees "... and N more skills. Ask me to list all skills." This keeps token overhead under LLM-07's 3,000-token budget for simple queries.

## Sources

### Primary (HIGH confidence)
- [/websites/rs_wasmtime](https://docs.rs/wasmtime/latest/) - Engine, Store, Config, fuel, epoch, Linker APIs
- [/websites/rs_wasmtime-wasi](https://docs.rs/wasmtime-wasi/latest/) - WasiCtxBuilder, preopened_dir, network controls
- [/bytecodealliance/wit-bindgen](https://github.com/bytecodealliance/wit-bindgen) - WIT guest bindings, generate! macro
- [wasmtime crates.io](https://crates.io/crates/wasmtime) - Version 40.x confirmed current

### Secondary (MEDIUM confidence)
- [Anthropic tool_use docs](https://docs.anthropic.com/en/docs/build-with-claude/tool-use) - tool_use/tool_result message format
- Existing Blufio codebase analysis: session.rs, types.rs, conditional.rs, anthropic types.rs

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - wasmtime is the de facto WASM runtime for Rust, version confirmed via crates.io
- Architecture: HIGH - patterns derived from wasmtime official docs and existing Blufio architecture
- Pitfalls: HIGH - well-documented in wasmtime ecosystem, verified against API docs

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (30 days - wasmtime is stable)
