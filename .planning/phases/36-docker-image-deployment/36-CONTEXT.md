# Phase 36: Docker Image & Deployment - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Multi-stage Dockerfile producing a minimal Docker image, docker-compose for single-command deployment, and multi-instance systemd template for running N Blufio instances. This phase covers containerization and service management — not orchestration (Kubernetes, Nomad) or CI/CD pipelines.

</domain>

<decisions>
## Implementation Decisions

### Image feature scope
- Single full image with all features compiled in (telegram, discord, slack, whatsapp, signal, irc, matrix, bridge, etc.)
- Users enable/disable adapters via config.toml, not by choosing different image variants
- Include ONNX runtime for local embedding support — full-featured image
- Image tagging: `blufio:latest`, `blufio:X.Y.Z`, `blufio:X.Y` (standard semver)
- Target architecture: amd64 only for now (ARM64 can be added later)

### Docker compose topology
- Blufio-only compose file — no bundled Prometheus, Grafana, or reverse proxy
- Users bring their own monitoring/proxy infrastructure
- Secrets via environment variables (`environment:` or `env_file:` in compose)
- Ship `.env.example` with all configurable vars, comments explaining each, and sensible defaults
- Production-only compose — devs use `cargo run` locally, Docker is for deployment

### Multi-instance systemd design
- Flexible template: instances can represent personas, platform sets, or tenants — template is agnostic
- Each instance binds to a different port (configurable in per-instance config)
- Instance management and directory structure at Claude's discretion

### Volume & config layout
- Container runs as non-root user (uid 65534, distroless nonroot)
- Directory structure, default config strategy, and plugin management approach at Claude's discretion

### Claude's Discretion
- Container internal directory layout (separate mounts vs single mount with subdirs)
- Whether to embed default config or require user-provided config.toml
- Plugin management inside container (bind mount vs named volume)
- Multi-instance config directory structure (/etc/blufio/instances/{name}/ vs /etc/blufio-{name}/)
- Instance management helpers (setup script vs manual with docs)
- ONNX runtime handling in multi-stage Docker build
- Build target choice (musl static vs glibc) based on dependency compatibility

</decisions>

<specifics>
## Specific Ideas

- Base image specified in requirements: `distroless/static-debian12:nonroot`
- Health check should use existing `/health` endpoint (unauthenticated, already implemented)
- Existing `contrib/blufio.service` is the more hardened template (Type=notify, memory limits, sd_notify integration)
- `release-musl` cargo profile already exists with optimizations (lto=true, opt-level="s", panic="abort", strip="symbols")

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `contrib/blufio.service`: Hardened systemd unit with Type=notify, watchdog, memory limits, security hardening — basis for template
- `deploy/blufio.service`: Simpler systemd unit — can be deprecated in favor of contrib version
- `blufio-agent/src/sdnotify.rs`: Full sd_notify integration (READY=1, STOPPING=1, WATCHDOG pings) — already works with Type=notify
- `blufio-gateway/src/server.rs`: `/health` endpoint (unauthenticated) ready for Docker health checks

### Established Patterns
- `bundled-sqlcipher-vendored-openssl`: SQLCipher compiles OpenSSL in — no runtime OpenSSL dependency
- `rustls` for TLS in reqwest and ort — no system TLS libraries needed at runtime
- `ort` with `download-binaries` and `copy-dylibs` — ONNX Runtime downloads at build time, copies shared libs
- `wasmtime` for WASM plugin execution — links to system libs
- `tikv-jemallocator` — custom allocator, needs to work in container environment
- Feature flags in `crates/blufio/Cargo.toml` control which adapters compile in

### Integration Points
- `BLUFIO_CONFIG` env var sets config path (used in existing systemd units)
- `RUST_LOG` env var controls log verbosity
- `/etc/blufio/environment` used as EnvironmentFile in contrib service
- `/var/lib/blufio` is the established working directory for data
- Port binding configured in config.toml (gateway section)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 36-docker-image-deployment*
*Context gathered: 2026-03-07*
