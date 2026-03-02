# Plan 07-02 Summary: WASM Sandbox and Skill System

## Status: COMPLETE

## What was built

### Task 1: SkillManifest parsing and real core types
- **`crates/blufio-core/src/types.rs`**: Replaced placeholder skill types with real structs: `SkillManifest`, `SkillCapabilities`, `NetworkCapability`, `FilesystemCapability`, `SkillResources`, `SkillInvocation`, `SkillResult`
- **`crates/blufio-skill/src/manifest.rs`**: TOML manifest parser with intermediate structs (`ManifestFile`, `SkillSection`, `CapabilitiesSection`, `NetworkSection`, `FilesystemSection`, `ResourcesSection`, `WasmSection`), validation (name alphanumeric+hyphens+underscores, non-empty), default resource values (1B fuel, 16MB memory, 5s epoch)
- 9 manifest tests

### Task 2: WasmSkillRuntime with sandboxed execution
- **`crates/blufio-skill/src/sandbox.rs`**: `WasmSkillRuntime` with per-invocation wasmtime `Store`, fuel metering, epoch interruption, and capability-gated host functions
  - Engine shared across invocations; modules compiled once at load time
  - Host functions: `log`, `get_input_len`, `get_input`, `set_output` (always available); `http_request`, `read_file`, `write_file`, `get_env` (capability-gated per manifest)
  - Epoch ticker: background tokio task increments `engine.increment_epoch()` every 1 second
  - WASM execution runs on `tokio::task::spawn_blocking` to prevent blocking the epoch ticker
  - Error detection uses `{e:#}` format to check full error chain for "all fuel consumed" or "wasm trap: interrupt"
- 9 sandbox tests including WAT inline WASM: creation, fuel exhaustion, epoch timeout, log output, minimal invoke

### Task 3: SkillStore, SkillConfig, scaffold, V5 migration
- **`crates/blufio-storage/migrations/V5__skill_registry.sql`**: `installed_skills` table with name, version, description, author, wasm_path, manifest_toml, capabilities_json, verification_status, installed_at, updated_at
- **`crates/blufio-config/src/model.rs`**: `SkillConfig` section with `skills_dir`, `default_fuel` (1B), `default_memory_mb` (16), `default_epoch_timeout_secs` (5), `max_skills_in_prompt` (20), `enabled` (false)
- **`crates/blufio-skill/src/store.rs`**: `SkillStore` with SQLite CRUD: `install()`, `remove()`, `get()`, `list()`. Uses `tokio_rusqlite::Connection` with explicit error type annotations
- **`crates/blufio-skill/src/scaffold.rs`**: `scaffold_skill()` generates Cargo.toml (cdylib), src/lib.rs (run() export), skill.toml (manifest). Validates name, prevents duplicate directories
- 6 store tests, 8 scaffold tests

## Key decisions
- **No WASI context**: Simplified by using raw host functions instead of WASI preopened directories. HTTP, file, and env access is handled through custom host function stubs that will be fully implemented in future iterations
- **spawn_blocking for WASM**: Synchronous WASM execution blocks the tokio thread, preventing epoch ticker advancement. Solved by running WASM on a blocking thread via `tokio::task::spawn_blocking`
- **wasmtime::Error not StdError**: wasmtime v40's `Error` (really `anyhow::Error`) doesn't implement `std::error::Error`, so we use `source: None` in `BlufioError::Skill` and include the error in the message string
- **Error chain matching**: wasmtime v40 wraps trap messages in "error while executing at wasm backtrace:" prefix. Used `{e:#}` (alternate Display) to get the full chain and match against "all fuel consumed" (fuel) and "wasm trap: interrupt" (epoch)

## Verification
- `cargo test -p blufio-skill`: 53 tests pass (21 tool/builtin + 9 manifest + 9 sandbox + 6 store + 8 scaffold)
- `cargo test -p blufio-config`: 30 tests pass (8 unit + 21 integration + 1 doctest)
- `cargo test -p blufio-storage`: 26 tests pass
- `cargo check --workspace`: clean compilation

## Files modified/created
- `Cargo.toml` (workspace): added wasmtime, wasmtime-wasi workspace deps
- `Cargo.lock`: updated with wasmtime ecosystem
- `crates/blufio-core/src/types.rs`: replaced placeholder skill types
- `crates/blufio-config/src/model.rs`: added SkillConfig section
- `crates/blufio-skill/Cargo.toml`: added wasmtime, wasmtime-wasi, anyhow, chrono, rusqlite, tokio-rusqlite, wat (dev)
- `crates/blufio-skill/src/lib.rs`: added sandbox, scaffold, store modules
- `crates/blufio-skill/src/manifest.rs` (new): TOML manifest parser
- `crates/blufio-skill/src/sandbox.rs` (new): WasmSkillRuntime
- `crates/blufio-skill/src/scaffold.rs` (new): project generator
- `crates/blufio-skill/src/store.rs` (new): SQLite skill registry
- `crates/blufio-storage/migrations/V5__skill_registry.sql` (new): installed_skills table
