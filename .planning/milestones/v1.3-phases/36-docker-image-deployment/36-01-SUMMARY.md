# Plan 36-01: Multi-stage Dockerfile and docker-compose — Summary

**Status:** Complete
**Duration:** ~10 min
**Tasks:** 2/2

## What was built

1. **`blufio healthcheck` CLI subcommand** (`crates/blufio/src/healthcheck.rs`) — Connects to the gateway `/health` endpoint with a 5-second timeout, exits 0 if healthy, 1 if not. Designed for Docker HEALTHCHECK directives in distroless images where no shell or curl is available.

2. **Multi-stage Dockerfile** (`Dockerfile`) — Three-stage build using cargo-chef for dependency layer caching:
   - Stage 1 (chef): Prepares dependency recipe from Cargo.toml/Cargo.lock
   - Stage 2 (builder): Compiles with `--all-features` using `rust:1.85-bookworm`
   - Stage 3 (runtime): `gcr.io/distroless/cc-debian12:nonroot` with binary + ONNX .so files
   - Uses `cc-debian12` (not `static-debian12`) because ONNX Runtime ships glibc-linked shared libraries

3. **`.dockerignore`** — Excludes target/, .git/, .planning/, .claude/, docs/, etc.

4. **`docker-compose.yml`** — Single-service deployment with named volume (`blufio-data`), read-only config bind mount, env_file support, and HEALTHCHECK using `blufio healthcheck`.

5. **`deploy/.env.example`** — Documents all configurable environment variables: deployment settings, AI provider API keys, channel tokens, gateway auth, and vault password.

## Key decisions

- Used `gcr.io/distroless/cc-debian12:nonroot` instead of `static-debian12` because ONNX Runtime and wasmtime ship glibc-linked .so files
- Healthcheck uses dedicated CLI subcommand (no curl/wget needed in distroless)
- cargo-chef saves ~15 min on rebuilds by caching the dependency compilation layer
- Blufio-only compose file (no bundled monitoring/proxy per user decision)

## Requirements covered

- INFRA-04: Docker multi-stage build producing minimal image
- INFRA-05: docker-compose.yml with volume mounts, env injection, and health check

## Files

| File | Action |
|------|--------|
| `crates/blufio/src/healthcheck.rs` | Created |
| `crates/blufio/src/main.rs` | Modified (added healthcheck module + command) |
| `Dockerfile` | Created |
| `.dockerignore` | Created |
| `docker-compose.yml` | Created |
| `deploy/.env.example` | Created |
