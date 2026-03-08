# Plan 36-02: Multi-instance systemd template — Summary

**Status:** Complete
**Duration:** ~5 min
**Tasks:** 2/2

## What was built

1. **`contrib/blufio@.service`** — Systemd template unit for running N Blufio instances on the same host. Based on the hardened `contrib/blufio.service` with `%i` substitutions for instance-specific paths:
   - Config: `/etc/blufio/instances/%i/config.toml`
   - Data: `/var/lib/blufio/instances/%i/`
   - Environment: `/etc/blufio/instances/%i/environment`
   - Hooks: `/var/lib/blufio/instances/%i/hooks/`
   - Preserves Type=notify, WatchdogSec=30, all security hardening

2. **`contrib/blufio-instance-setup.sh`** — Helper script that creates the directory structure and default configuration for a new instance:
   - Validates instance name (alphanumeric + hyphens/underscores)
   - Creates config and data directories
   - Generates default config.toml with instance-specific port
   - Creates environment file template for API keys
   - Sets proper ownership (blufio:blufio) and permissions (640 for environment)
   - Supports `--port` flag for multi-instance port assignment

## Key decisions

- Instance directories under `/etc/blufio/instances/{name}/` (config) and `/var/lib/blufio/instances/{name}/` (data)
- Each instance gets its own SQLite database, plugin directory, and hook scripts
- Environment file has restricted permissions (640, root:blufio) since it contains secrets
- Template is instance-agnostic: can represent personas, platform sets, or tenants

## Requirements covered

- INFRA-07: Multi-instance systemd template (blufio@.service) with per-instance config

## Files

| File | Action |
|------|--------|
| `contrib/blufio@.service` | Created |
| `contrib/blufio-instance-setup.sh` | Created |
