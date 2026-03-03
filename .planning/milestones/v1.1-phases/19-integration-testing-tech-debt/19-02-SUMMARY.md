---
phase: 19-integration-testing-tech-debt
plan: 02
status: completed
requirements_completed: [DEBT-01, DEBT-02, DEBT-03]
commit: a87ac07
---

## Summary

Resolved three tech debt items: sessions endpoint, SessionActor constructor, and systemd deployment.

### What Changed

**Task 1: Wire StorageAdapter into GET /v1/sessions (DEBT-01)**
- Added `storage: Option<Arc<dyn StorageAdapter + Send + Sync>>` to `GatewayState`
- Added `set_storage()` method to `GatewayChannel` (mirrors `set_mcp_router()` pattern)
- Replaced hardcoded empty list in `get_sessions()` with real `storage.list_sessions()` query
- Returns 500 with error message on storage failure, empty list if storage not wired
- Called `gateway.set_storage(storage.clone())` in `serve.rs`

**Task 2A: SessionActor refactoring (DEBT-03)**
- Created `SessionActorConfig` struct grouping all 15 constructor arguments
- Changed `SessionActor::new` to accept `SessionActorConfig` instead of individual args
- Removed `#[allow(clippy::too_many_arguments)]` annotation
- Updated all 4 call sites: `lib.rs` (2 sites), `delegation.rs`, `harness.rs`

**Task 2B: systemd unit file (DEBT-02)**
- Created `deploy/blufio.service` with security hardening
- Dedicated `blufio` user, 60s stop timeout for LLM drain, Restart=on-failure
- ProtectSystem=strict, ProtectHome=yes, NoNewPrivileges, PrivateTmp, etc.

### Files Modified
- `crates/blufio-gateway/src/server.rs` - StorageAdapter field in GatewayState
- `crates/blufio-gateway/src/handlers.rs` - Real session query in get_sessions
- `crates/blufio-gateway/src/lib.rs` - set_storage() method + storage field
- `crates/blufio-agent/src/session.rs` - SessionActorConfig struct + new constructor
- `crates/blufio-agent/src/lib.rs` - Updated 2 SessionActor::new call sites
- `crates/blufio-agent/src/delegation.rs` - Updated SessionActor::new call site
- `crates/blufio-test-utils/src/harness.rs` - Updated SessionActor::new call site
- `crates/blufio/src/serve.rs` - Wire storage into gateway
- `deploy/blufio.service` - New systemd unit file

### Verification
- `cargo test -p blufio-agent -p blufio-gateway -p blufio-test-utils` passes (81 tests)
- `cargo test --test e2e` passes (12 E2E tests)
- No `#[allow(clippy::too_many_arguments)]` in session.rs
- `deploy/blufio.service` exists with [Unit], [Service], [Install] sections
