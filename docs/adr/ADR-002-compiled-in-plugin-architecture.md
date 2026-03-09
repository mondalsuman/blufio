# ADR-002: Compiled-in Plugin Architecture

**Status:** Accepted
**Date:** 2026-02-XX (decided, Phase 1) / 2026-03-09 (recorded)

## Context and Problem Statement

Blufio uses an everything-is-a-plugin architecture with 7 adapter traits. All plugins are compiled into the binary. This is an existing decision being formally documented; we chose compiled-in during Phase 1 (February 2026). The question is whether plugins should be compiled-in, dynamically loaded, or run as subprocesses.

## Decision Drivers

- Compile-time guarantees: trait coherence, exhaustive matching, lifetime checking
- Single-binary deployment model (copy one file)
- Security audit simplicity -- all code visible at build time

## Considered Options

1. **Compiled-in** (current) -- all plugins statically linked
2. **libloading dynamic** -- load .so/.dylib at runtime
3. **Subprocess/IPC plugins** -- separate process per plugin

## Decision Outcome

We chose compiled-in because it provides compile-time safety (trait coherence, exhaustive matching, lifetime verification), single-binary deployment, and full auditability. Dynamic loading deferred until community or build-time pressure justifies ABI complexity.

### Trait Hierarchy

```
PluginAdapter (base: name, version, adapter_type, health_check, shutdown)
  |
  +-- ChannelAdapter       crates/blufio-core/src/traits/channel.rs
  +-- ProviderAdapter      crates/blufio-core/src/traits/provider.rs
  +-- StorageAdapter       crates/blufio-core/src/traits/storage.rs
  +-- EmbeddingAdapter     crates/blufio-core/src/traits/embedding.rs
  +-- ObservabilityAdapter crates/blufio-core/src/traits/observability.rs
  +-- AuthAdapter          crates/blufio-core/src/traits/auth.rs
  +-- SkillRuntimeAdapter  crates/blufio-core/src/traits/skill.rs
```

All traits use `#[async_trait]` enabling dyn dispatch:

```rust
#[async_trait]
pub trait PluginAdapter: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> semver::Version;
    fn adapter_type(&self) -> AdapterType;
    async fn health_check(&self) -> Result<HealthStatus, BlufioError>;
    async fn shutdown(&self) -> Result<(), BlufioError>;
}
```

### Built-in Plugin Table

| Crate | Adapter Trait | Gating |
|-------|--------------|--------|
| blufio-telegram | ChannelAdapter | Default |
| blufio-discord | ChannelAdapter | Feature-gated |
| blufio-slack | ChannelAdapter | Feature-gated |
| blufio-whatsapp | ChannelAdapter | Feature-gated |
| blufio-signal | ChannelAdapter | Feature-gated |
| blufio-irc | ChannelAdapter | Feature-gated |
| blufio-matrix | ChannelAdapter | Feature-gated |
| blufio-gateway | ChannelAdapter | Default |
| blufio-anthropic | ProviderAdapter | Default |
| blufio-openai | ProviderAdapter | Feature-gated |
| blufio-ollama | ProviderAdapter | Feature-gated |
| blufio-openrouter | ProviderAdapter | Feature-gated |
| blufio-gemini | ProviderAdapter | Feature-gated |
| blufio-storage | StorageAdapter | Default |
| blufio-memory | EmbeddingAdapter | Default |
| blufio-prometheus | ObservabilityAdapter | Default |
| blufio-auth-keypair | AuthAdapter | Default |

### WASM Skills vs Native Plugins

WASM skills are third-party code running in a sandboxed trust boundary (wasmtime with fuel/memory/epoch limits). Native plugins are system extensions compiled into the binary with full trust. `blufio plugin install <name>` searches the built-in catalog (hardcoded) and enables a compiled-in plugin -- no network downloads. This would change with dynamic loading.

## Consequences

- **Good:** Compile-time trait checking, exhaustive matching, single binary, no ABI concerns
- **Bad:** Recompile required for new plugins, all plugins contribute to binary size
- **Neutral:** MCP client provides partial extensibility without recompilation (tools only, not full adapter replacement)

### PluginRegistry Architecture

PluginRegistry holds `PluginEntry` structs keyed by name, each containing a manifest, status (Enabled/Disabled/NotConfigured), and an optional factory. PluginHost manages the lifecycle.

## Migration Roadmap

**Phase 1 (current):** All plugins compiled-in. PluginRegistry holds manifests and factories. `blufio plugin install` enables built-in plugins. Full compile-time safety.

**Phase 2 (trigger: community contributors or prohibitive build time):** Official plugins loaded via libloading. Host and plugin share abi_stable types. Code signing required for loaded .so files. Capability sandboxing needed.

**Phase 3 (trigger: third-party ecosystem demand):** Third-party dynamic plugins with full isolation. Plugin manifest with semver compatibility. Hot-reload with state preservation. Registry/marketplace distribution.

### ABI Stability Challenge

Rust has no stable ABI. The abi_stable crate (0.11.3, last updated October 2023) works around this but requires matching versions across host and plugin. Low maintenance activity validates deferring dynamic loading.

### Security Model Comparison

| Aspect | Compiled-in | Dynamic (libloading) |
|--------|------------|---------------------|
| Trust model | Audited at build time | Needs code signing |
| Memory safety | Full Rust guarantees | ABI boundary risks |
| Capability control | Trait system enforces | Needs runtime sandboxing |
| Supply chain | cargo-deny audit | Plugin registry verification |
| Update mechanism | Recompile binary | Hot-reload possible |

## Related ADRs

- [ADR-001](ADR-001-ort-onnx-inference.md) -- ORT .so files require distroless cc, affects deployment model for dynamic plugins

## References

- DOC-02 requirement
- PROJECT.md Key Decisions: "Everything-is-a-plugin"
