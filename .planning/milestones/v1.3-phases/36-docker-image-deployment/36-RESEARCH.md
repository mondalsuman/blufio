# Phase 36: Docker Image & Deployment — Research

**Researched:** 2026-03-07
**Status:** Complete

## 1. Build Strategy: Multi-Stage Dockerfile

### Target: glibc-based build (not musl)

The project has complex native dependencies that make musl static linking problematic:
- **wasmtime 40**: Links against system libraries, requires glibc
- **ONNX Runtime (ort 2.0.0-rc.11)**: `download-binaries` downloads pre-built glibc-linked `.so` files; no musl-compatible builds available
- **tikv-jemallocator 0.6**: Works with both, but glibc is the tested path
- **sd-notify 0.4**: Pure Rust, no issues
- **SQLCipher (bundled-sqlcipher-vendored-openssl)**: Compiles OpenSSL from source — works with both, but glibc avoids cross-compilation issues

**Decision: Use `debian:bookworm-slim` as builder, `gcr.io/distroless/cc-debian12:nonroot` as runtime.**

The `cc-debian12` variant (not `static-debian12`) is required because ONNX Runtime and wasmtime ship shared libraries (.so files) that need glibc. The distroless `cc` variant includes glibc and libgcc but nothing else — still minimal (~20MB base).

Note: The CONTEXT.md mentions `distroless/static-debian12:nonroot` but this won't work with ONNX Runtime shared libraries. The `cc-debian12:nonroot` variant is the correct choice for this dependency set.

### Multi-Stage Build Design

```
Stage 1: "chef" — cargo-chef for dependency caching
  - FROM rust:1.85-bookworm
  - Install cargo-chef
  - Copy Cargo.toml/Cargo.lock files
  - Run cargo chef prepare

Stage 2: "builder" — compile with cached deps
  - FROM rust:1.85-bookworm
  - Install build deps: pkg-config, libclang-dev, cmake, protobuf-compiler
  - Copy recipe from chef stage
  - Run cargo chef cook --release (caches deps)
  - Copy full source
  - Run cargo build --release --all-features
  - Locate and collect ONNX shared libraries

Stage 3: "runtime" — minimal production image
  - FROM gcr.io/distroless/cc-debian12:nonroot
  - Copy binary from builder
  - Copy ONNX .so files to /usr/lib/
  - Set LD_LIBRARY_PATH if needed
  - EXPOSE 3000
  - HEALTHCHECK
  - ENTRYPOINT ["/blufio", "serve"]
```

### Cargo-chef for Layer Caching

Without cargo-chef, every source change recompiles all dependencies (~400+ crates). Cargo-chef creates a "recipe" from Cargo.toml/Cargo.lock that lets Docker cache the dependency compilation layer separately.

### ONNX Runtime Handling

The `ort` crate with `download-binaries` + `copy-dylibs` features:
1. Downloads ONNX Runtime pre-built binaries during `cargo build`
2. Copies `.so` files next to the output binary
3. These .so files must be copied to the runtime image and be on `LD_LIBRARY_PATH`

Key: The .so files appear in `target/release/` alongside the binary. They need to be collected and placed in `/usr/lib/` in the runtime container.

### Build Dependencies Required

For compilation in the builder stage:
- `pkg-config` — for finding system libraries
- `libclang-dev` — for bindgen (used by rusqlite/SQLCipher)
- `cmake` — for building SQLCipher's vendored OpenSSL
- `make`, `gcc`, `g++` — standard build tools (included in rust:bookworm)
- Rust 1.85+ (workspace minimum)

### .dockerignore

Essential for build context size:
```
target/
.git/
.planning/
.claude/
*.md
!CHANGELOG.md
```

## 2. Docker Compose Design

### Single-Service Compose File

```yaml
services:
  blufio:
    image: blufio:latest  # or build: .
    container_name: blufio
    restart: unless-stopped
    ports:
      - "${BLUFIO_PORT:-3000}:3000"
    volumes:
      - blufio-data:/data
      - ./config:/config:ro
    environment:
      - BLUFIO_CONFIG=/config/config.toml
      - RUST_LOG=${RUST_LOG:-blufio=info}
    env_file:
      - .env
    healthcheck:
      test: ["CMD", "/blufio", "healthcheck"]  # or wget/curl to /health
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s
```

### Health Check Approach

The distroless image has no shell, curl, or wget. Options:
1. **Built-in healthcheck subcommand** — Add `blufio healthcheck` CLI command that hits `http://localhost:3000/health` and exits 0/1. This is the cleanest approach for distroless.
2. **Use /health endpoint directly** — Would need a static binary (wget/curl) copied into the image.

**Recommended: Option 1** — `blufio healthcheck` subcommand. One line of code, no extra binaries in the image. The `/health` endpoint already exists and is unauthenticated.

### Volume Layout

Container-internal directories:
- `/data` — SQLite database, ONNX models, plugin store (read-write)
- `/config` — config.toml (read-only mount)
- `/data/plugins` — WASM plugin files (subdirectory of data volume)

### .env.example

Must document all configurable environment variables:
- `BLUFIO_PORT` — host port mapping (default: 3000)
- `BLUFIO_CONFIG` — config file path inside container
- `RUST_LOG` — log level
- `ANTHROPIC_API_KEY` — Anthropic API key
- `OPENAI_API_KEY` — OpenAI API key
- `TELEGRAM_BOT_TOKEN` — Telegram bot token
- `DISCORD_TOKEN` — Discord bot token
- All other platform-specific secrets

## 3. Systemd Template Design

### Template Pattern: blufio@.service

Systemd template units use `%i` for the instance identifier and `%I` for unescaped version:

```ini
[Service]
ExecStart=/usr/local/bin/blufio serve
Environment=BLUFIO_CONFIG=/etc/blufio/instances/%i/config.toml
WorkingDirectory=/var/lib/blufio/instances/%i
```

Usage: `systemctl start blufio@personal`, `systemctl start blufio@work`

### Per-Instance Directory Structure

```
/etc/blufio/instances/{name}/
  config.toml          # Instance-specific config (different port, adapters)
  environment          # Instance-specific env vars (API keys)

/var/lib/blufio/instances/{name}/
  blufio.db            # Instance database
  plugins/             # Instance plugins
  models/              # Instance ONNX models
```

### Key Adaptations from contrib/blufio.service

The existing `contrib/blufio.service` is excellent and should be the basis:
- Keep `Type=notify` with `NotifyAccess=main` (sd_notify already implemented)
- Keep `WatchdogSec=30` (watchdog pinger already works)
- Keep all security hardening (NoNewPrivileges, PrivateTmp, etc.)
- Adapt `ReadWritePaths` to instance-specific path: `/var/lib/blufio/instances/%i`
- Adapt `EnvironmentFile` to instance-specific: `/etc/blufio/instances/%i/environment`
- Keep memory limits (`MemoryMax=256M`, `MemoryHigh=200M`)

### Instance Management

A setup helper script simplifies creating new instances:
```bash
#!/bin/bash
# blufio-instance-setup.sh <instance-name>
# Creates directory structure and copies default config
```

## 4. Healthcheck CLI Subcommand

Need to add a `blufio healthcheck` subcommand that:
1. Reads the config to determine the gateway port
2. Makes HTTP GET to `http://127.0.0.1:{port}/health`
3. Exits 0 if 200 OK, exits 1 otherwise
4. Has a 5-second timeout

This is minimal code: ~20 lines in the CLI module.

## 5. Risk Analysis

### Low Risk
- Systemd template: straightforward adaptation of existing service file
- Docker compose: standard single-service compose
- .env.example: documentation only

### Medium Risk
- **ONNX Runtime .so files**: Need to correctly locate and copy them in Docker build. The `copy-dylibs` feature puts them next to the binary, but the exact filenames may vary.
- **Image size**: Full build with all features + ONNX Runtime could be 200-400MB. Acceptable for a full-featured image but worth noting.

### Mitigations
- Use `find target/release -name "libonnxruntime*"` in Dockerfile to locate .so files
- Document expected image size in comments

## 6. Files to Create/Modify

### New Files
1. `Dockerfile` — Multi-stage build
2. `.dockerignore` — Build context exclusions
3. `docker-compose.yml` — Single-service deployment
4. `deploy/.env.example` — Environment variable template
5. `contrib/blufio@.service` — Systemd template unit
6. `contrib/blufio-instance-setup.sh` — Instance setup helper

### Modified Files
7. `crates/blufio/src/main.rs` or CLI module — Add `healthcheck` subcommand

## RESEARCH COMPLETE
