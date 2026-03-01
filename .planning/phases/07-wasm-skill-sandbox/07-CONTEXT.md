# Phase 7: WASM Skill Sandbox - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Third-party skills execute in isolated WASM sandboxes with capability manifests, fuel metering, and memory limits. The agent discovers skills progressively and executes them safely alongside built-in tools (bash, HTTP, file I/O). The plugin host system (Phase 8) and production hardening (Phase 9) are separate phases.

</domain>

<decisions>
## Implementation Decisions

### Built-in tools design
- Unified registry: built-in tools and WASM skills share the same tool_use interface. Built-ins run natively instead of in WASM but are registered identically
- Full bash access with no restrictions — this is a personal agent on a single-user VPS
- HTTP requests use existing SSRF prevention (private IP blocking) and TLS enforcement from blufio-security — no additional guards
- Full filesystem access for the file I/O tool, consistent with bash access
- Tool calling needs wiring into the Anthropic provider (tool_use/tool_result flow doesn't exist yet)

### WASM skill interface
- Minimal host functions exposed to WASM skills: log(level, msg), http_request(url, method, headers, body), read_file(path), write_file(path, data), get_env(key)
- Each host function gated by capability manifest — if the skill doesn't declare network permission, http_request is unavailable
- Rust-only SDK at launch; `blufio skill init` scaffolds a Rust project
- Other languages that compile to WASM can work but without official templates

### Permission & capability model
- Install-time approval: when installing a skill, its capabilities are displayed and the user approves once (like Android app permissions)
- Path-scoped filesystem permissions: skills declare specific directories they need (e.g., read: ["~/.config/myapp"], write: ["/tmp/skill-output"])
- Domain-scoped network permissions: skills declare which domains they need access to (e.g., ["api.github.com", "*.openai.com"])
- Conservative resource defaults: 16MB memory max, 1 billion fuel instructions (~1-2 seconds CPU), 5 second epoch timeout
- Resource limits configurable per-skill in manifest

### Progressive skill discovery
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

</decisions>

<specifics>
## Specific Ideas

- Built-in tools and WASM skills should be indistinguishable from the LLM's perspective — same tool_use interface
- Capability manifest should feel like a package.json or Cargo.toml — declarative, readable, familiar to developers
- The SkillProvider reuses the ConditionalProvider pattern established by MemoryProvider in Phase 5

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `SkillRuntimeAdapter` trait (blufio-core/src/traits/skill.rs): Already defined with `invoke()` and `list_skills()` methods — needs real implementation
- `SkillManifest`, `SkillInvocation`, `SkillResult` types (blufio-core/src/types.rs): Placeholder structs with `_placeholder: ()` — need real fields
- `ConditionalProvider` trait (blufio-context/src/conditional.rs): Established pattern for injecting context — reuse for skill discovery
- `MemoryProvider` (blufio-memory/src/provider.rs): Reference implementation of ConditionalProvider — model the SkillProvider after this
- SSRF prevention and TLS enforcement (blufio-security): Ready to use for HTTP tool guards

### Established Patterns
- Three-zone context assembly (static/conditional/dynamic): Skills inject into conditional (one-liners) and dynamic (full SKILL.md on invocation)
- Adapter trait pattern: All adapters extend `PluginAdapter` base trait with `async_trait` for dynamic dispatch
- SQLite persistence via blufio-storage: Skill registry can use the same storage patterns
- Integration wiring in `serve.rs` and `shell.rs`: New components get wired up here

### Integration Points
- Anthropic provider (blufio-anthropic): Tool calling (tool_use/tool_result) needs to be wired — currently skipped ("Phase 3 doesn't use tool calling")
- SessionActor (blufio-agent/src/session.rs): FSM loop needs to handle tool_use blocks from LLM responses, execute tools, return tool_result
- Context engine (blufio-context): Add SkillProvider as ConditionalProvider via `add_conditional_provider()`
- CLI (blufio/src/main.rs): Add `blufio skill` subcommand for init/list/install/remove

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 07-wasm-skill-sandbox*
*Context gathered: 2026-03-01*
