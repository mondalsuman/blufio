---
phase: 36-docker-image-deployment
verified: 2026-03-07T16:50:00Z
status: passed
score: 3/3 must-haves verified (Docker build UNVERIFIED -- no daemon available)
re_verification: false
---

# Phase 36: Docker Image & Deployment Verification Report

**Phase Goal:** Docker multi-stage build producing minimal image, docker-compose with volumes/env/healthcheck, multi-instance systemd template
**Verified:** 2026-03-07
**Status:** PASSED (static verification only -- Docker daemon not available)
**Re-verification:** No -- initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Multi-stage Dockerfile producing minimal image with distroless base | VERIFIED (static) | `Dockerfile` lines 1-58: Three-stage build (chef -> builder -> runtime). Stage 1: `rust:1.85-bookworm AS chef` with cargo-chef for recipe preparation. Stage 2: `rust:1.85-bookworm AS builder` compiles with `--all-features`. Stage 3: `gcr.io/distroless/cc-debian12:nonroot` copies only binary + ONNX libs. Uses `cc-debian12` (not `static-debian12`) because ONNX Runtime ships glibc-linked .so files. `COPY --from=builder /src/target/release/blufio /blufio` and `COPY --from=builder /ort-libs/ /usr/lib/`. HEALTHCHECK directive uses `/blufio healthcheck`. ENTRYPOINT/CMD properly set. **Note: Actual Docker image build UNVERIFIED -- Docker daemon not available on build machine.** |
| 2 | docker-compose.yml with volume mounts, env injection, and health check | VERIFIED (static) | `docker-compose.yml` lines 1-39: Service `blufio` with `image: blufio:latest`. Named volume `blufio-data:/data` for persistent storage. Read-only config bind mount `./config:/config:ro`. Environment variables `BLUFIO_CONFIG`, `RUST_LOG` with defaults. `env_file` directive with `required: false` for .env loading. Healthcheck: `["/blufio", "healthcheck"]` with 30s interval, 5s timeout, 3 retries, 10s start period. Port mapping `${BLUFIO_PORT:-3000}:3000`. Restart policy `unless-stopped`. |
| 3 | Multi-instance systemd template with per-instance config and data directories | VERIFIED | `contrib/blufio@.service` lines 1-61: Template unit using `%i` substitution throughout. Config: `BLUFIO_CONFIG=/etc/blufio/instances/%i/config.toml` (line 40). Data: `WorkingDirectory=/var/lib/blufio/instances/%i` (line 37). Environment: `EnvironmentFile=-/etc/blufio/instances/%i/environment` (line 41). Security hardening: `NoNewPrivileges=yes`, `PrivateTmp=yes`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/blufio/instances/%i`. Type=notify, WatchdogSec=30. Memory limits: MemoryMax=256M, MemoryHigh=200M. `contrib/blufio-instance-setup.sh` (133 lines): validates instance name, creates directories, generates default config.toml with instance-specific port, sets permissions (640 for environment file). |

**Score:** 3/3 truths verified (Docker build itself UNVERIFIED due to missing daemon -- static analysis only)

---

## Required Artifacts

### Plan 01: Multi-stage Dockerfile and docker-compose

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `Dockerfile` | Multi-stage build with cargo-chef and distroless runtime | VERIFIED (static) | 58 lines; 3 stages (chef, builder, runtime); cargo-chef dependency caching; `gcr.io/distroless/cc-debian12:nonroot`; ONNX .so collection; HEALTHCHECK directive; EXPOSE 3000 |
| `.dockerignore` | Excludes build artifacts, docs, planning files | VERIFIED | 14 lines; excludes target/, .git/, .planning/, .claude/, .agents/, .github/, contrib/, deploy/, docs/, *.md (except CHANGELOG.md) |
| `docker-compose.yml` | Single-service deployment with volumes, env, healthcheck | VERIFIED (static) | 39 lines; named volume `blufio-data`, read-only config mount, env_file with required: false, healthcheck using `/blufio healthcheck`, configurable port |
| `deploy/.env.example` | Documents all configurable environment variables | VERIFIED | 69 lines; documents all provider API keys (Anthropic, OpenAI, Ollama, OpenRouter, Gemini), channel tokens (Telegram, Discord, Slack, WhatsApp, Signal, IRC, Matrix), gateway bearer token, vault password |
| `crates/blufio/src/healthcheck.rs` | Healthcheck CLI subcommand for Docker HEALTHCHECK | VERIFIED | 55 lines; `run_healthcheck()` connects to gateway `/health` endpoint with 5s timeout; exits 0 (healthy) or returns error (unhealthy); designed for distroless (no shell/curl needed) |
| `crates/blufio/src/main.rs` | healthcheck module and Healthcheck command | VERIFIED | `mod healthcheck;` declaration (line 20); `Healthcheck` in Commands enum |

### Plan 02: Multi-instance systemd template

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `contrib/blufio@.service` | Systemd template unit with %i substitutions | VERIFIED | 61 lines; %i used for instance-specific paths throughout; config at `/etc/blufio/instances/%i/config.toml`; data at `/var/lib/blufio/instances/%i/`; security hardened; Type=notify with WatchdogSec=30 |
| `contrib/blufio-instance-setup.sh` | Instance creation helper script | VERIFIED | 133 lines; validates instance name (alphanumeric + hyphens/underscores); creates config and data directories; generates default config.toml with instance-specific port; sets ownership (blufio:blufio) and permissions (640 for environment) |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| Dockerfile Stage 3 | distroless cc-debian12:nonroot | `FROM` directive | WIRED | Line 44: `FROM gcr.io/distroless/cc-debian12:nonroot` -- cc variant for glibc ONNX libs |
| Dockerfile HEALTHCHECK | `/blufio healthcheck` | CMD directive | WIRED | Line 54-55: `CMD ["/blufio", "healthcheck"]` -- no shell required in distroless |
| docker-compose healthcheck | `/blufio healthcheck` | test directive | WIRED | Line 31: `test: ["/blufio", "healthcheck"]` -- matches Dockerfile pattern |
| docker-compose volumes | blufio-data named volume | volumes section | WIRED | Line 22: `blufio-data:/data` persistent storage + line 23: `./config:/config:ro` read-only config |
| docker-compose env_file | .env file | env_file directive | WIRED | Lines 28-29: `path: .env, required: false` -- optional secrets injection |
| deploy/.env.example | docker-compose environment | documentation | WIRED | All env vars documented with comments; maps to docker-compose `environment:` section |
| blufio@.service %i | Instance config directory | EnvironmentFile | WIRED | Line 41: `EnvironmentFile=-/etc/blufio/instances/%i/environment` |
| blufio@.service %i | Instance data directory | WorkingDirectory | WIRED | Line 37: `WorkingDirectory=/var/lib/blufio/instances/%i` |
| blufio-instance-setup.sh | blufio@.service paths | Directory creation | WIRED | Script creates exact directories referenced by service template |
| healthcheck.rs | BlufioConfig gateway.host + daemon.health_port | Config struct | WIRED | Line 20-22: reads host and port from config to construct health URL |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| INFRA-04 | 36-01 | Docker multi-stage build producing minimal image | SATISFIED (static verification) | Three-stage Dockerfile: chef (recipe), builder (compile), runtime (distroless cc-debian12:nonroot). COPY --from=builder for binary and ONNX libs only. Minimal final image with no build tools. **Docker build itself UNVERIFIED** -- Docker daemon not available on build machine. Dockerfile syntax and structure verified statically. |
| INFRA-05 | 36-01 | docker-compose.yml with volume mounts, env injection, and health check | SATISFIED | Named volume `blufio-data:/data` for persistence; `./config:/config:ro` read-only bind mount; `env_file` with `required: false`; environment variables with defaults; HEALTHCHECK using `blufio healthcheck` subcommand; port mapping with default 3000; restart unless-stopped |
| INFRA-07 | 36-02 | Multi-instance systemd template (blufio@.service) | SATISFIED | Template unit with `%i` throughout: `/etc/blufio/instances/%i/config.toml`, `/var/lib/blufio/instances/%i/`, `/etc/blufio/instances/%i/environment`. Security hardened (NoNewPrivileges, PrivateTmp, ProtectSystem=strict, ReadWritePaths restricted to instance dir). Instance setup script creates directories, generates config, sets permissions. |

All 3 requirements verified. INFRA-04 Docker build is UNVERIFIED due to missing Docker daemon -- this is expected per context decision and does not block verification.

---

## Docker Build Status

**Status: UNVERIFIED (static analysis only)**

The Docker image build cannot be verified because Docker daemon is not available on this build machine. The following was verified statically:

- Dockerfile syntax is valid (proper FROM, COPY, RUN, EXPOSE, HEALTHCHECK, ENTRYPOINT, CMD directives)
- Multi-stage build structure is correct (3 stages: chef -> builder -> runtime)
- COPY --from=builder paths reference correct build output locations
- distroless base image tag is correct (`cc-debian12:nonroot` for glibc compatibility)
- HEALTHCHECK command matches the healthcheck.rs implementation
- .dockerignore excludes unnecessary files

**What requires Docker daemon to verify:**
- Actual image build succeeds
- Image size is within 200MB soft target
- ONNX .so files are correctly copied to /usr/lib/
- Binary runs correctly inside distroless container
- Healthcheck endpoint responds inside container

Per context decision: "Build failure flagged as UNVERIFIED, doesn't block rest of verification."

---

## Anti-Patterns Found

No anti-patterns detected.

Scanned Dockerfile, docker-compose.yml, blufio@.service, and blufio-instance-setup.sh for:
- TODO/FIXME/HACK comments: none found
- Hardcoded secrets or passwords: none found
- Running as root in container: `nonroot` user in distroless
- Running as root in systemd: `User=blufio, Group=blufio` configured
- Environment file permissions too open: 640 with root:blufio ownership
- Missing security hardening: all expected systemd directives present

---

## Test Summary

| Module | Tests | Status |
|--------|-------|--------|
| healthcheck (compilation smoke test) | 1 | PASSED |
| **Total** | **1** | **PASSED** |

Note: The healthcheck module has a compilation smoke test. Full healthcheck testing requires a running gateway, which is an integration-level concern. The healthcheck.rs code path (HTTP GET to /health with 5s timeout) is straightforward and the reqwest client usage is correct.

---

## Gaps Summary

**One gap:** Docker image build not verified (no daemon). This is expected and documented. All artifacts exist, are syntactically correct, and follow established patterns. Static analysis confirms Dockerfile structure, docker-compose.yml configuration, and systemd template are complete and well-formed.

No other gaps. All 3 observable truths verified (2 statically, 1 fully). All 8 artifacts exist and are substantive. All 10 key links are wired. All 3 requirements satisfied.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-executor)_
