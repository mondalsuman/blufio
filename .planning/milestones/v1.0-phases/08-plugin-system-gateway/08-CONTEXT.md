# Phase 8: Plugin System & Gateway - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Plugin host loads adapter plugins implementing the seven adapter traits (Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime), a CLI manages the plugin lifecycle, and an HTTP/WebSocket gateway enables API access alongside channel messaging. Third-party/runtime plugin loading is out of scope — this phase covers the built-in adapter framework only.

</domain>

<decisions>
## Implementation Decisions

### Plugin loading mechanism
- Compile-time Cargo features — each adapter is a feature flag, default build includes all 6 standard adapters
- PluginRegistry pattern at startup: adapters register themselves, serve.rs queries the registry for the active Channel/Provider/Storage/etc. instead of hardcoding
- PluginAdapter base trait stays as-is (name, version, adapter_type, health_check, shutdown) — no changes needed
- Third-party plugin support deferred — built-in framework only for this phase
- Users can build custom binaries with `--no-default-features --features api,anthropic,sqlite` for stripped-down deployments

### Plugin registry & discovery
- Plugin manifests extend skill.toml pattern (same format with adapter-specific fields: adapter_type, capabilities, config schema, min_blufio_version)
- `blufio plugin search` queries a hardcoded built-in catalog compiled into the binary — no network calls
- `blufio plugin install/remove` toggles enabled state in blufio.toml config (binary ships with all default adapters compiled in)
- `blufio plugin list` shows ALL compiled-in adapters with status: enabled/disabled/not-configured
- `blufio plugin update` is informational (binary updates are whole-binary)

### Gateway architecture
- HTTP/WebSocket gateway implemented as a ChannelAdapter — reuses entire agent loop, session management, tool pipeline
- Full REST API + SSE streaming: POST /v1/messages (with Accept: text/event-stream for streaming), GET /v1/sessions, GET /v1/health
- WebSocket at /ws supports bidirectional streaming — client sends messages anytime, server streams responses in real-time with typing indicators and partial responses
- Bearer token authentication from blufio.toml or vault
- Single axum server: /v1/* for API, /ws for WebSocket, /metrics for Prometheus — one port, different paths

### Default bundle strategy
- Keep existing crates (blufio-telegram, blufio-anthropic, blufio-storage) + add new crates (blufio-gateway, blufio-prometheus, blufio-auth-keypair)
- Each adapter crate is a Cargo feature of the main blufio binary, all optional with default features including all 6
- Prometheus adapter: standard /metrics endpoint with counters/gauges (messages processed, token usage, session counts, response latency, budget remaining)
- Auth adapter: bearer token validation backed by device keypair stored in vault, keypair auto-generated at first run

### Claude's Discretion
- PluginRegistry internal architecture and registration API
- Plugin manifest field names and exact TOML structure
- SSE streaming protocol details and event format
- WebSocket message frame format and protocol
- Prometheus metric naming conventions
- Device keypair generation algorithm (Ed25519 vs other)
- Error response format for gateway API
- Gateway crate internal module structure

</decisions>

<specifics>
## Specific Ideas

- Gateway API shape inspired by Anthropic's own Messages API (POST /v1/messages)
- `blufio plugin list` output similar to `systemctl list-unit-files` with status column
- Single-port design for simple personal deployment behind reverse proxy

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PluginAdapter` base trait (`blufio-core/src/traits/adapter.rs`): name, version, adapter_type, health_check, shutdown — all 7 adapter traits already defined
- `AdapterType` enum (`blufio-core/src/types.rs`): Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime
- `ChannelAdapter` trait (`blufio-core/src/traits/channel.rs`): connect, send, receive, edit_message, send_typing — gateway implements this
- WASM skill system (`blufio-skill`): SkillStore, ToolRegistry, manifest parsing, scaffold — manifest format can be extended for plugin.toml
- CLI clap structure (`blufio/src/main.rs`): Commands enum with Serve, Shell, Config, Skill — add Plugin subcommand
- `CostLedger` and `BudgetTracker` (`blufio-cost`): already tracks token usage, cost data available for Prometheus metrics

### Established Patterns
- `Arc<dyn Trait + Send + Sync>` for dynamic dispatch of adapters throughout agent loop
- `#[async_trait]` on all adapter traits for async method support
- Config via TOML with figment (`blufio-config`) — new plugin enable/disable config section fits here
- Vault for secret storage (`blufio-vault`) — bearer tokens stored here
- Graceful shutdown via `CancellationToken` — gateway needs to participate in shutdown
- `wasmtime` already in workspace dependencies for skill WASM runtime

### Integration Points
- `serve.rs` currently hardcodes adapter initialization — refactor to use PluginRegistry
- `AgentLoop::new()` already accepts `Box<dyn ChannelAdapter>` — gateway channel plugs in here
- `AgentLoop::run()` currently polls single channel — needs to support multiple channels (Telegram + Gateway)
- `blufio-config/model.rs` needs new sections: `[plugins]`, `[gateway]`, `[prometheus]`
- Cargo.toml workspace needs new member crates: blufio-gateway, blufio-prometheus, blufio-auth-keypair
- axum dependency needed (not currently in workspace)

</code_context>

<deferred>
## Deferred Ideas

- Third-party runtime plugin loading (dynamic libraries, WASM adapters) — v2 concern
- Challenge-response keypair authentication — future enhancement
- Structured JSON log export via ObservabilityAdapter — future phase
- GitHub/HTTP-based plugin registry for community plugins — v2
- Separate metrics port for production deployments — configurable in future

</deferred>

---

*Phase: 08-plugin-system-gateway*
*Context gathered: 2026-03-01*
