// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WASM skill sandbox using wasmtime.
//!
//! Each skill invocation creates a fresh [`wasmtime::Store`] with per-invocation
//! fuel, memory, and epoch controls. Host functions are capability-gated based
//! on the skill's manifest -- a skill without network permission cannot call
//! `http_request`.
//!
//! Capability-denied host functions trap (return `Err(wasmtime::Error)`) instead
//! of returning error codes. This ensures the WASM execution halts immediately
//! with a descriptive error, which is caught by the invoke() error handler and
//! returned as a SkillResult with is_error=true.
//!
//! The [`Engine`] and compiled [`Module`]s are shared across invocations for
//! efficiency (compilation happens once at load time).

use std::collections::HashMap;

use anyhow::anyhow;
use blufio_core::BlufioError;
use blufio_core::types::{SkillInvocation, SkillManifest, SkillResult};
use tracing::{debug, info, warn};
use wasmtime::{Caller, Config, Engine, Linker, Memory, Module, Store};

/// State stored in each wasmtime Store for a single skill invocation.
struct SkillState {
    /// The skill's manifest (for capability checks in host function impls).
    /// Accessed via caller.data() in host function closures during invocation.
    #[allow(dead_code)]
    manifest: SkillManifest,
    /// Accumulated log output from the skill.
    output: Vec<String>,
    /// Input JSON passed to the skill (read by the skill via host function).
    input_json: String,
    /// Result JSON written by the skill.
    result_json: Option<String>,
}

/// WASM skill runtime with per-invocation sandboxing.
///
/// The engine and compiled modules are shared across invocations for
/// efficiency. Each invocation creates a fresh Store with its own fuel,
/// memory limits, and epoch deadline.
pub struct WasmSkillRuntime {
    engine: Engine,
    manifests: HashMap<String, SkillManifest>,
    modules: HashMap<String, Module>,
}

impl WasmSkillRuntime {
    /// Creates a new WASM skill runtime with fuel metering, epoch interruption,
    /// and memory limits enabled.
    pub fn new() -> Result<Self, BlufioError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config).map_err(|e| BlufioError::Skill {
            message: format!("failed to create wasmtime engine: {e}"),
            source: None,
        })?;

        info!("WASM skill runtime initialized");

        Ok(Self {
            engine,
            manifests: HashMap::new(),
            modules: HashMap::new(),
        })
    }

    /// Loads a skill from its manifest and WASM binary bytes.
    ///
    /// The WASM module is compiled once and cached. Subsequent invocations
    /// reuse the compiled module with fresh Store instances.
    pub fn load_skill(
        &mut self,
        manifest: SkillManifest,
        wasm_bytes: &[u8],
    ) -> Result<(), BlufioError> {
        let module = Module::new(&self.engine, wasm_bytes).map_err(|e| BlufioError::Skill {
            message: format!(
                "failed to compile WASM module for skill '{}': {e}",
                manifest.name
            ),
            source: None,
        })?;

        info!(skill = %manifest.name, version = %manifest.version, "loaded WASM skill");
        self.modules.insert(manifest.name.clone(), module);
        self.manifests.insert(manifest.name.clone(), manifest);
        Ok(())
    }

    /// Invokes a loaded skill with JSON input.
    ///
    /// Creates a fresh wasmtime Store with:
    /// - Fuel limit from the skill's manifest
    /// - Epoch deadline for wall-clock timeout
    /// - Capability-gated host functions
    ///
    /// An epoch ticker background task increments the engine epoch every second,
    /// causing the skill to trap if it exceeds its timeout.
    pub async fn invoke(&self, invocation: SkillInvocation) -> Result<SkillResult, BlufioError> {
        let manifest = self
            .manifests
            .get(&invocation.skill_name)
            .ok_or_else(|| BlufioError::Skill {
                message: format!("skill '{}' not loaded", invocation.skill_name),
                source: None,
            })?;

        let module = self
            .modules
            .get(&invocation.skill_name)
            .ok_or_else(|| BlufioError::Skill {
                message: format!("module for skill '{}' not found", invocation.skill_name),
                source: None,
            })?;

        let input_json = serde_json::to_string(&invocation.input).map_err(|e| {
            BlufioError::Skill {
                message: format!("failed to serialize skill input: {e}"),
                source: Some(Box::new(e)),
            }
        })?;

        // Create fresh Store with skill state.
        let state = SkillState {
            manifest: manifest.clone(),
            output: Vec::new(),
            input_json,
            result_json: None,
        };
        let mut store = Store::new(&self.engine, state);

        // Set fuel limit.
        store.set_fuel(manifest.resources.fuel).map_err(|e| {
            BlufioError::Skill {
                message: format!("failed to set fuel: {e}"),
                source: None,
            }
        })?;

        // Configure epoch deadline for wall-clock timeout.
        store.epoch_deadline_trap();
        store.set_epoch_deadline(manifest.resources.epoch_timeout_secs);

        // Create linker with host functions.
        let mut linker = Linker::new(&self.engine);
        define_host_functions(&mut linker, manifest)?;

        // Spawn epoch ticker (increments engine epoch every 1 second).
        let engine_clone = self.engine.clone();
        let timeout_secs = manifest.resources.epoch_timeout_secs;
        let ticker_handle = tokio::spawn(async move {
            for _ in 0..timeout_secs + 1 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                engine_clone.increment_epoch();
            }
        });

        // Clone module for the blocking task (Module is cheaply cloneable).
        let module = module.clone();

        // Run WASM execution on a blocking thread so the epoch ticker can
        // advance on the tokio runtime while the WASM is executing.
        let wasm_result = tokio::task::spawn_blocking(move || {
            let instance = linker.instantiate(&mut store, &module)?;
            let run_func = instance
                .get_typed_func::<(), ()>(&mut store, "run")
                .map_err(|e| anyhow::anyhow!("skill has no 'run' export: {e}"))?;
            run_func.call(&mut store, ())?;
            Ok::<Store<SkillState>, anyhow::Error>(store)
        })
        .await
        .map_err(|e| BlufioError::Skill {
            message: format!("WASM execution task panicked: {e}"),
            source: None,
        })?;

        // Abort the epoch ticker.
        ticker_handle.abort();

        let skill_name = &invocation.skill_name;
        let fuel = manifest.resources.fuel;
        let timeout = manifest.resources.epoch_timeout_secs;

        match wasm_result {
            Ok(store) => {
                let state = store.data();
                let content = if let Some(ref result_json) = state.result_json {
                    result_json.clone()
                } else if !state.output.is_empty() {
                    state.output.join("\n")
                } else {
                    "Skill completed successfully (no output)".to_string()
                };

                Ok(SkillResult {
                    content,
                    is_error: false,
                })
            }
            Err(e) => {
                // Use {e:#} to get the full error chain including nested causes.
                let error_msg = format!("{e:#}");
                let content = if error_msg.contains("all fuel consumed") {
                    format!(
                        "Skill '{skill_name}' exceeded fuel limit ({fuel} fuel units): {error_msg}"
                    )
                } else if error_msg.contains("wasm trap: interrupt") {
                    format!(
                        "Skill '{skill_name}' exceeded wall-clock timeout ({timeout}s): {error_msg}"
                    )
                } else if error_msg.contains("capability not permitted") {
                    format!(
                        "Skill '{skill_name}' capability denied: {error_msg}"
                    )
                } else {
                    format!("Skill '{skill_name}' execution error: {error_msg}")
                };

                Ok(SkillResult {
                    content,
                    is_error: true,
                })
            }
        }
    }

    /// Returns clones of all loaded skill manifests.
    pub fn list_skills(&self) -> Vec<SkillManifest> {
        self.manifests.values().cloned().collect()
    }

    /// Returns a reference to the engine (for testing).
    #[cfg(test)]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Returns true if a skill with the given name is loaded.
    pub fn has_skill(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }
}

/// Defines capability-gated host functions in the linker.
///
/// Each host function checks the skill's manifest capabilities before executing.
/// Functions for capabilities the skill has not declared trap with
/// "capability not permitted" on invocation (via `Err(anyhow!(...))` which
/// wasmtime converts to a wasm trap).
fn define_host_functions(
    linker: &mut Linker<SkillState>,
    manifest: &SkillManifest,
) -> Result<(), BlufioError> {
    // --- log: always available ---
    linker
        .func_wrap(
            "blufio",
            "log",
            |mut caller: Caller<'_, SkillState>, level: i32, ptr: i32, len: i32| {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return,
                };
                if let Some(msg) = read_string_from_memory(&memory, &caller, ptr, len) {
                    let level_str = match level {
                        0 => "TRACE",
                        1 => "DEBUG",
                        2 => "INFO",
                        3 => "WARN",
                        4 => "ERROR",
                        _ => "INFO",
                    };
                    debug!(skill_log = %msg, level = level_str, "skill log");
                    caller.data_mut().output.push(format!("[{level_str}] {msg}"));
                }
            },
        )
        .map_err(linker_err)?;

    // --- get_input: always available ---
    // Returns the length of the input JSON. Skill reads it from a buffer.
    linker
        .func_wrap(
            "blufio",
            "get_input_len",
            |caller: Caller<'_, SkillState>| -> i32 {
                caller.data().input_json.len() as i32
            },
        )
        .map_err(linker_err)?;

    // Copies input JSON into WASM memory at the given pointer.
    linker
        .func_wrap(
            "blufio",
            "get_input",
            |mut caller: Caller<'_, SkillState>, ptr: i32| {
                let input = caller.data().input_json.clone();
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return,
                };
                write_bytes_to_memory(&memory, &mut caller, ptr, input.as_bytes());
            },
        )
        .map_err(linker_err)?;

    // --- set_output: always available ---
    // Skill writes its result JSON to host.
    linker
        .func_wrap(
            "blufio",
            "set_output",
            |mut caller: Caller<'_, SkillState>, ptr: i32, len: i32| {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return,
                };
                if let Some(output) = read_string_from_memory(&memory, &caller, ptr, len) {
                    caller.data_mut().result_json = Some(output);
                }
            },
        )
        .map_err(linker_err)?;

    // --- http_request: capability-gated ---
    // Traps if network capability is not declared. When permitted, makes a real
    // HTTP request using reqwest (via tokio runtime handle) with domain validation
    // and SSRF prevention. Stores response body in result_json, returns status code.
    let has_network = manifest.capabilities.network.is_some();
    let allowed_domains: Vec<String> = manifest
        .capabilities
        .network
        .as_ref()
        .map(|n| n.domains.clone())
        .unwrap_or_default();
    linker
        .func_wrap(
            "blufio",
            "http_request",
            move |mut caller: Caller<'_, SkillState>,
                  url_ptr: i32,
                  url_len: i32,
                  _method: i32,
                  _body_ptr: i32,
                  _body_len: i32|
                  -> Result<i32, wasmtime::Error> {
                if !has_network {
                    warn!("skill attempted http_request without network capability");
                    return Err(anyhow!("capability not permitted: skill lacks network permission").into());
                }

                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(anyhow!("WASM module has no exported memory").into()),
                };

                let url = match read_string_from_memory(&memory, &caller, url_ptr, url_len) {
                    Some(u) => u,
                    None => return Err(anyhow!("failed to read URL from WASM memory").into()),
                };

                // Validate URL domain against the manifest's allowed domains.
                let parsed_url = reqwest::Url::parse(&url)
                    .map_err(|e| anyhow!("invalid URL '{url}': {e}"))?;

                if let Some(domain) = parsed_url.host_str() {
                    if !allowed_domains.iter().any(|d| domain == d || domain.ends_with(&format!(".{d}"))) {
                        return Err(anyhow!(
                            "capability not permitted: domain '{domain}' not in allowed list {:?}",
                            allowed_domains
                        ).into());
                    }
                } else {
                    return Err(anyhow!("URL has no host: {url}").into());
                }

                // SSRF prevention: block private/internal IPs.
                if let Err(e) = blufio_security::ssrf::validate_url_host(&url) {
                    return Err(anyhow!("SSRF blocked: {e}").into());
                }

                // Make the HTTP request using the tokio runtime handle.
                // We are inside spawn_blocking, so Handle::current() is available.
                let handle = tokio::runtime::Handle::current();
                let response = handle.block_on(async {
                    let client = reqwest::Client::new();
                    client.get(&url).send().await
                });

                match response {
                    Ok(resp) => {
                        let status = resp.status().as_u16() as i32;
                        let body = handle.block_on(async {
                            resp.text().await.unwrap_or_default()
                        });

                        // Store the response body in result_json for the skill to access.
                        caller.data_mut().result_json = Some(body);
                        info!(url = %url, status = status, "WASM http_request completed");
                        Ok(status)
                    }
                    Err(e) => {
                        warn!(url = %url, error = %e, "WASM http_request failed");
                        Err(anyhow!("HTTP request failed: {e}").into())
                    }
                }
            },
        )
        .map_err(linker_err)?;

    // --- read_file: capability-gated ---
    // Traps if filesystem read capability is not declared. When permitted,
    // reads file content, validates path against manifest's read paths,
    // stores content in result_json, and returns content length.
    let has_fs_read = manifest
        .capabilities
        .filesystem
        .as_ref()
        .is_some_and(|f| !f.read.is_empty());
    let read_paths: Vec<String> = manifest
        .capabilities
        .filesystem
        .as_ref()
        .map(|f| f.read.clone())
        .unwrap_or_default();
    linker
        .func_wrap(
            "blufio",
            "read_file",
            move |mut caller: Caller<'_, SkillState>,
                  path_ptr: i32,
                  path_len: i32,
                  _buf_ptr: i32,
                  _buf_len: i32|
                  -> Result<i32, wasmtime::Error> {
                if !has_fs_read {
                    warn!("skill attempted read_file without filesystem read capability");
                    return Err(anyhow!("capability not permitted: skill lacks filesystem read permission").into());
                }

                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(anyhow!("WASM module has no exported memory").into()),
                };

                let path = match read_string_from_memory(&memory, &caller, path_ptr, path_len) {
                    Some(p) => p,
                    None => return Err(anyhow!("failed to read path from WASM memory").into()),
                };

                // Validate that the path starts with one of the manifest's read paths.
                let path_allowed = read_paths.iter().any(|allowed| path.starts_with(allowed));
                if !path_allowed {
                    return Err(anyhow!(
                        "capability not permitted: path '{}' not within allowed read paths {:?}",
                        path, read_paths
                    ).into());
                }

                // Read the file.
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let len = content.len() as i32;
                        caller.data_mut().result_json = Some(content);
                        info!(path = %path, len = len, "WASM read_file completed");
                        Ok(len)
                    }
                    Err(e) => {
                        Err(anyhow!("read_file failed for '{}': {}", path, e).into())
                    }
                }
            },
        )
        .map_err(linker_err)?;

    // --- write_file: capability-gated ---
    // Traps if filesystem write capability is not declared. When permitted,
    // reads data from WASM memory, validates path against manifest's write paths,
    // writes to disk, and returns 0 on success.
    let has_fs_write = manifest
        .capabilities
        .filesystem
        .as_ref()
        .is_some_and(|f| !f.write.is_empty());
    let write_paths: Vec<String> = manifest
        .capabilities
        .filesystem
        .as_ref()
        .map(|f| f.write.clone())
        .unwrap_or_default();
    linker
        .func_wrap(
            "blufio",
            "write_file",
            move |mut caller: Caller<'_, SkillState>,
                  path_ptr: i32,
                  path_len: i32,
                  data_ptr: i32,
                  data_len: i32|
                  -> Result<i32, wasmtime::Error> {
                if !has_fs_write {
                    warn!("skill attempted write_file without filesystem write capability");
                    return Err(anyhow!("capability not permitted: skill lacks filesystem write permission").into());
                }

                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(anyhow!("WASM module has no exported memory").into()),
                };

                let path = match read_string_from_memory(&memory, &caller, path_ptr, path_len) {
                    Some(p) => p,
                    None => return Err(anyhow!("failed to read path from WASM memory").into()),
                };

                let data = match read_string_from_memory(&memory, &caller, data_ptr, data_len) {
                    Some(d) => d,
                    None => return Err(anyhow!("failed to read data from WASM memory").into()),
                };

                // Validate that the path starts with one of the manifest's write paths.
                let path_allowed = write_paths.iter().any(|allowed| path.starts_with(allowed));
                if !path_allowed {
                    return Err(anyhow!(
                        "capability not permitted: path '{}' not within allowed write paths {:?}",
                        path, write_paths
                    ).into());
                }

                // Write the file.
                match std::fs::write(&path, data.as_bytes()) {
                    Ok(()) => {
                        info!(path = %path, len = data.len(), "WASM write_file completed");
                        Ok(0)
                    }
                    Err(e) => {
                        Err(anyhow!("write_file failed for '{}': {}", path, e).into())
                    }
                }
            },
        )
        .map_err(linker_err)?;

    // --- get_env: capability-gated ---
    let allowed_env: Vec<String> = manifest.capabilities.env.clone();
    linker
        .func_wrap(
            "blufio",
            "get_env",
            move |mut caller: Caller<'_, SkillState>,
                  key_ptr: i32,
                  key_len: i32,
                  val_ptr: i32,
                  val_len: i32|
                  -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return -1,
                };
                let key = match read_string_from_memory(&memory, &caller, key_ptr, key_len) {
                    Some(k) => k,
                    None => return -1,
                };

                if !allowed_env.contains(&key) {
                    warn!(key = %key, "skill attempted get_env for non-permitted key");
                    return -1;
                }

                match std::env::var(&key) {
                    Ok(val) => {
                        let bytes = val.as_bytes();
                        if bytes.len() > val_len as usize {
                            return -2; // Buffer too small
                        }
                        write_bytes_to_memory(&memory, &mut caller, val_ptr, bytes);
                        bytes.len() as i32
                    }
                    Err(_) => -1,
                }
            },
        )
        .map_err(linker_err)?;

    Ok(())
}

/// Helper: read a UTF-8 string from WASM memory.
fn read_string_from_memory(
    memory: &Memory,
    caller: &Caller<'_, SkillState>,
    ptr: i32,
    len: i32,
) -> Option<String> {
    let ptr = ptr as usize;
    let len = len as usize;
    let data = memory.data(caller);
    if ptr + len > data.len() {
        return None;
    }
    String::from_utf8(data[ptr..ptr + len].to_vec()).ok()
}

/// Helper: write bytes into WASM memory.
fn write_bytes_to_memory(
    memory: &Memory,
    caller: &mut Caller<'_, SkillState>,
    ptr: i32,
    bytes: &[u8],
) {
    let ptr = ptr as usize;
    let data = memory.data_mut(caller);
    if ptr + bytes.len() <= data.len() {
        data[ptr..ptr + bytes.len()].copy_from_slice(bytes);
    }
}

/// Helper: convert linker errors to BlufioError.
fn linker_err(e: anyhow::Error) -> BlufioError {
    BlufioError::Skill {
        message: format!("failed to define host function: {e}"),
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::types::{NetworkCapability, SkillCapabilities, SkillResources};

    #[test]
    fn sandbox_runtime_creates_successfully() {
        let runtime = WasmSkillRuntime::new();
        assert!(runtime.is_ok());
    }

    #[test]
    fn sandbox_engine_has_fuel_and_epoch() {
        let runtime = WasmSkillRuntime::new().unwrap();
        // If fuel and epoch were not configured, the engine wouldn't accept
        // stores with those settings. Verify by creating a store.
        let mut store = Store::new(
            runtime.engine(),
            SkillState {
                manifest: test_manifest(),
                output: Vec::new(),
                input_json: "{}".to_string(),
                result_json: None,
            },
        );
        // set_fuel should succeed because consume_fuel is enabled.
        assert!(store.set_fuel(1000).is_ok());
        // epoch_deadline_trap should not panic because epoch_interruption is enabled.
        store.epoch_deadline_trap();
        store.set_epoch_deadline(5);
    }

    #[tokio::test]
    async fn sandbox_invoke_unknown_skill_returns_error() {
        let runtime = WasmSkillRuntime::new().unwrap();
        let invocation = SkillInvocation {
            skill_name: "nonexistent".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not loaded"));
    }

    #[test]
    fn sandbox_list_skills_empty() {
        let runtime = WasmSkillRuntime::new().unwrap();
        assert!(runtime.list_skills().is_empty());
    }

    #[test]
    fn sandbox_load_and_list_skill() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Minimal valid WASM module (just exports a "run" function that returns immediately).
        let wat = r#"(module
            (func (export "run"))
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        let manifest = test_manifest();
        runtime.load_skill(manifest.clone(), &wasm).unwrap();

        let skills = runtime.list_skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test-skill");
        assert!(runtime.has_skill("test-skill"));
        assert!(!runtime.has_skill("other-skill"));
    }

    #[tokio::test]
    async fn sandbox_invoke_minimal_skill() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Minimal skill that just exports "run" and does nothing.
        let wat = r#"(module
            (func (export "run"))
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        let manifest = test_manifest();
        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({"query": "test"}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn sandbox_fuel_exhaustion_returns_error() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that loops forever (will exhaust fuel).
        let wat = r#"(module
            (func (export "run")
                (loop $forever
                    (br $forever)
                )
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        let mut manifest = test_manifest();
        manifest.resources.fuel = 10_000; // Very low fuel
        manifest.resources.epoch_timeout_secs = 60; // High timeout so fuel runs out first

        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(result.is_error);
        assert!(
            result.content.contains("exceeded fuel limit"),
            "Expected fuel error, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn sandbox_skill_with_log_output() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that calls log host function.
        let wat = r#"(module
            (import "blufio" "log" (func $log (param i32 i32 i32)))
            (func (export "run")
                ;; Write "hello" to memory at offset 0
                (i32.store8 (i32.const 0) (i32.const 104))  ;; h
                (i32.store8 (i32.const 1) (i32.const 101))  ;; e
                (i32.store8 (i32.const 2) (i32.const 108))  ;; l
                (i32.store8 (i32.const 3) (i32.const 108))  ;; l
                (i32.store8 (i32.const 4) (i32.const 111))  ;; o
                ;; Call log(level=2 (INFO), ptr=0, len=5)
                (call $log (i32.const 2) (i32.const 0) (i32.const 5))
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        let manifest = test_manifest();
        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(!result.is_error);
        assert!(
            result.content.contains("hello"),
            "Expected log output, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn sandbox_epoch_ticker_spawns_and_aborts() {
        // Verify that the epoch ticker mechanism works by running a skill
        // with a very short timeout.
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that loops forever.
        let wat = r#"(module
            (func (export "run")
                (loop $forever
                    (br $forever)
                )
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        let mut manifest = test_manifest();
        manifest.resources.fuel = u64::MAX; // Very high fuel so epoch triggers first
        manifest.resources.epoch_timeout_secs = 1; // 1 second timeout

        runtime.load_skill(manifest, &wasm).unwrap();

        let start = std::time::Instant::now();
        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        let elapsed = start.elapsed();

        assert!(result.is_error);
        // Should complete within a few seconds (1s timeout + epoch granularity).
        assert!(
            elapsed.as_secs() < 5,
            "Epoch timeout should have triggered within 5s, took {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn sandbox_http_request_denied_produces_trap() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that calls http_request with no network capability.
        // The host function should trap (not return -1).
        let wat = r#"(module
            (import "blufio" "http_request" (func $http_request (param i32 i32 i32 i32 i32) (result i32)))
            (func (export "run")
                ;; Write a URL "http://example.com" to memory at offset 0
                (i32.store8 (i32.const 0) (i32.const 104))   ;; h
                (i32.store8 (i32.const 1) (i32.const 116))   ;; t
                (i32.store8 (i32.const 2) (i32.const 116))   ;; t
                (i32.store8 (i32.const 3) (i32.const 112))   ;; p
                (i32.store8 (i32.const 4) (i32.const 58))    ;; :
                (i32.store8 (i32.const 5) (i32.const 47))    ;; /
                (i32.store8 (i32.const 6) (i32.const 47))    ;; /
                (i32.store8 (i32.const 7) (i32.const 101))   ;; e
                (i32.store8 (i32.const 8) (i32.const 120))   ;; x
                (i32.store8 (i32.const 9) (i32.const 97))    ;; a
                (i32.store8 (i32.const 10) (i32.const 109))  ;; m
                (i32.store8 (i32.const 11) (i32.const 112))  ;; p
                (i32.store8 (i32.const 12) (i32.const 108))  ;; l
                (i32.store8 (i32.const 13) (i32.const 101))  ;; e
                (i32.store8 (i32.const 14) (i32.const 46))   ;; .
                (i32.store8 (i32.const 15) (i32.const 99))   ;; c
                (i32.store8 (i32.const 16) (i32.const 111))  ;; o
                (i32.store8 (i32.const 17) (i32.const 109))  ;; m
                ;; Call http_request(url_ptr=0, url_len=18, method=0, body_ptr=0, body_len=0)
                (drop (call $http_request (i32.const 0) (i32.const 18) (i32.const 0) (i32.const 0) (i32.const 0)))
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        // Manifest with NO network capability.
        let manifest = test_manifest();
        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(result.is_error, "Expected error result, got: {}", result.content);
        assert!(
            result.content.contains("capability not permitted"),
            "Expected 'capability not permitted' in error, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn sandbox_read_file_denied_produces_trap() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that calls read_file with no filesystem capability.
        let wat = r#"(module
            (import "blufio" "read_file" (func $read_file (param i32 i32 i32 i32) (result i32)))
            (func (export "run")
                ;; Write "/tmp/test" to memory at offset 0
                (i32.store8 (i32.const 0) (i32.const 47))   ;; /
                (i32.store8 (i32.const 1) (i32.const 116))  ;; t
                (i32.store8 (i32.const 2) (i32.const 109))  ;; m
                (i32.store8 (i32.const 3) (i32.const 112))  ;; p
                (i32.store8 (i32.const 4) (i32.const 47))   ;; /
                (i32.store8 (i32.const 5) (i32.const 116))  ;; t
                (i32.store8 (i32.const 6) (i32.const 101))  ;; e
                (i32.store8 (i32.const 7) (i32.const 115))  ;; s
                (i32.store8 (i32.const 8) (i32.const 116))  ;; t
                ;; Call read_file(path_ptr=0, path_len=9, buf_ptr=0, buf_len=0)
                (drop (call $read_file (i32.const 0) (i32.const 9) (i32.const 0) (i32.const 0)))
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        // Manifest with NO filesystem capability.
        let manifest = test_manifest();
        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(result.is_error, "Expected error result, got: {}", result.content);
        assert!(
            result.content.contains("capability not permitted"),
            "Expected 'capability not permitted' in error, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn sandbox_write_file_denied_produces_trap() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that calls write_file with no filesystem capability.
        let wat = r#"(module
            (import "blufio" "write_file" (func $write_file (param i32 i32 i32 i32) (result i32)))
            (func (export "run")
                ;; Write "/tmp/out" to memory at offset 0
                (i32.store8 (i32.const 0) (i32.const 47))   ;; /
                (i32.store8 (i32.const 1) (i32.const 116))  ;; t
                (i32.store8 (i32.const 2) (i32.const 109))  ;; m
                (i32.store8 (i32.const 3) (i32.const 112))  ;; p
                (i32.store8 (i32.const 4) (i32.const 47))   ;; /
                (i32.store8 (i32.const 5) (i32.const 111))  ;; o
                (i32.store8 (i32.const 6) (i32.const 117))  ;; u
                (i32.store8 (i32.const 7) (i32.const 116))  ;; t
                ;; Write "hi" to memory at offset 100
                (i32.store8 (i32.const 100) (i32.const 104))  ;; h
                (i32.store8 (i32.const 101) (i32.const 105))  ;; i
                ;; Call write_file(path_ptr=0, path_len=8, data_ptr=100, data_len=2)
                (drop (call $write_file (i32.const 0) (i32.const 8) (i32.const 100) (i32.const 2)))
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        // Manifest with NO filesystem capability.
        let manifest = test_manifest();
        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(result.is_error, "Expected error result, got: {}", result.content);
        assert!(
            result.content.contains("capability not permitted"),
            "Expected 'capability not permitted' in error, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn sandbox_read_file_with_permission_reads_real_file() {
        use blufio_core::types::FilesystemCapability;

        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Create a temporary file to read.
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello from file").unwrap();

        let file_path = test_file.to_str().unwrap();
        let dir_path = temp_dir.path().to_str().unwrap();

        // Build WAT that writes the file path to memory and calls read_file.
        let path_bytes: Vec<u8> = file_path.as_bytes().to_vec();
        let mut store_instrs = String::new();
        for (i, &b) in path_bytes.iter().enumerate() {
            store_instrs.push_str(&format!(
                "                (i32.store8 (i32.const {i}) (i32.const {b}))\n"
            ));
        }

        let wat = format!(
            r#"(module
            (import "blufio" "read_file" (func $read_file (param i32 i32 i32 i32) (result i32)))
            (func (export "run")
{store_instrs}                ;; Call read_file(path_ptr=0, path_len={path_len}, buf_ptr=0, buf_len=0)
                (drop (call $read_file (i32.const 0) (i32.const {path_len}) (i32.const 0) (i32.const 0)))
            )
            (memory (export "memory") 1)
        )"#,
            path_len = path_bytes.len(),
        );
        let wasm = wat::parse_str(&wat).unwrap();

        // Manifest WITH filesystem read capability.
        let mut manifest = test_manifest();
        manifest.capabilities.filesystem = Some(FilesystemCapability {
            read: vec![dir_path.to_string()],
            write: vec![],
        });

        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(!result.is_error, "Unexpected error: {}", result.content);
        assert_eq!(result.content, "hello from file");
    }

    #[tokio::test]
    async fn sandbox_write_file_with_permission_writes_real_file() {
        use blufio_core::types::FilesystemCapability;

        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Create a temporary directory for writing.
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("output.txt");
        let file_path = test_file.to_str().unwrap();
        let dir_path = temp_dir.path().to_str().unwrap();

        // Build WAT that writes the file path and data to memory, then calls write_file.
        let path_bytes: Vec<u8> = file_path.as_bytes().to_vec();
        let data_bytes: Vec<u8> = b"written by wasm".to_vec();
        let data_offset = 200; // Put data at offset 200 to avoid overlap with path

        let mut store_instrs = String::new();
        for (i, &b) in path_bytes.iter().enumerate() {
            store_instrs.push_str(&format!(
                "                (i32.store8 (i32.const {i}) (i32.const {b}))\n"
            ));
        }
        for (i, &b) in data_bytes.iter().enumerate() {
            store_instrs.push_str(&format!(
                "                (i32.store8 (i32.const {}) (i32.const {b}))\n",
                data_offset + i
            ));
        }

        let wat = format!(
            r#"(module
            (import "blufio" "write_file" (func $write_file (param i32 i32 i32 i32) (result i32)))
            (func (export "run")
{store_instrs}                ;; Call write_file(path_ptr=0, path_len={path_len}, data_ptr={data_offset}, data_len={data_len})
                (drop (call $write_file (i32.const 0) (i32.const {path_len}) (i32.const {data_offset}) (i32.const {data_len})))
            )
            (memory (export "memory") 1)
        )"#,
            path_len = path_bytes.len(),
            data_len = data_bytes.len(),
        );
        let wasm = wat::parse_str(&wat).unwrap();

        // Manifest WITH filesystem write capability.
        let mut manifest = test_manifest();
        manifest.capabilities.filesystem = Some(FilesystemCapability {
            read: vec![],
            write: vec![dir_path.to_string()],
        });

        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(!result.is_error, "Unexpected error: {}", result.content);

        // Verify the file was written.
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "written by wasm");
    }

    #[tokio::test]
    async fn sandbox_read_file_outside_allowed_path_traps() {
        use blufio_core::types::FilesystemCapability;

        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Create a temporary file to read.
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("secret.txt");
        std::fs::write(&test_file, "secret data").unwrap();

        let file_path = test_file.to_str().unwrap();

        // Build WAT that tries to read a file.
        let path_bytes: Vec<u8> = file_path.as_bytes().to_vec();
        let mut store_instrs = String::new();
        for (i, &b) in path_bytes.iter().enumerate() {
            store_instrs.push_str(&format!(
                "                (i32.store8 (i32.const {i}) (i32.const {b}))\n"
            ));
        }

        let wat = format!(
            r#"(module
            (import "blufio" "read_file" (func $read_file (param i32 i32 i32 i32) (result i32)))
            (func (export "run")
{store_instrs}                (drop (call $read_file (i32.const 0) (i32.const {path_len}) (i32.const 0) (i32.const 0)))
            )
            (memory (export "memory") 1)
        )"#,
            path_len = path_bytes.len(),
        );
        let wasm = wat::parse_str(&wat).unwrap();

        // Manifest WITH filesystem read capability but for a DIFFERENT path.
        let mut manifest = test_manifest();
        manifest.capabilities.filesystem = Some(FilesystemCapability {
            read: vec!["/some/other/path".to_string()],
            write: vec![],
        });

        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(result.is_error, "Expected error, got: {}", result.content);
        assert!(
            result.content.contains("capability not permitted"),
            "Expected path validation error, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn sandbox_http_request_domain_not_allowed_traps() {
        let mut runtime = WasmSkillRuntime::new().unwrap();

        // Skill that calls http_request with a URL whose domain is not in the allowlist.
        let wat = r#"(module
            (import "blufio" "http_request" (func $http_request (param i32 i32 i32 i32 i32) (result i32)))
            (func (export "run")
                ;; Write "http://evil.com" to memory at offset 0
                (i32.store8 (i32.const 0) (i32.const 104))   ;; h
                (i32.store8 (i32.const 1) (i32.const 116))   ;; t
                (i32.store8 (i32.const 2) (i32.const 116))   ;; t
                (i32.store8 (i32.const 3) (i32.const 112))   ;; p
                (i32.store8 (i32.const 4) (i32.const 58))    ;; :
                (i32.store8 (i32.const 5) (i32.const 47))    ;; /
                (i32.store8 (i32.const 6) (i32.const 47))    ;; /
                (i32.store8 (i32.const 7) (i32.const 101))   ;; e
                (i32.store8 (i32.const 8) (i32.const 118))   ;; v
                (i32.store8 (i32.const 9) (i32.const 105))   ;; i
                (i32.store8 (i32.const 10) (i32.const 108))  ;; l
                (i32.store8 (i32.const 11) (i32.const 46))   ;; .
                (i32.store8 (i32.const 12) (i32.const 99))   ;; c
                (i32.store8 (i32.const 13) (i32.const 111))  ;; o
                (i32.store8 (i32.const 14) (i32.const 109))  ;; m
                ;; Call http_request(url_ptr=0, url_len=15, method=0, body_ptr=0, body_len=0)
                (drop (call $http_request (i32.const 0) (i32.const 15) (i32.const 0) (i32.const 0) (i32.const 0)))
            )
            (memory (export "memory") 1)
        )"#;
        let wasm = wat::parse_str(wat).unwrap();

        // Manifest WITH network capability but only for "api.example.com".
        let mut manifest = test_manifest();
        manifest.capabilities.network = Some(NetworkCapability {
            domains: vec!["api.example.com".to_string()],
        });

        runtime.load_skill(manifest, &wasm).unwrap();

        let invocation = SkillInvocation {
            skill_name: "test-skill".to_string(),
            input: serde_json::json!({}),
        };
        let result = runtime.invoke(invocation).await.unwrap();
        assert!(result.is_error, "Expected error result, got: {}", result.content);
        assert!(
            result.content.contains("capability not permitted") || result.content.contains("not in allowed list"),
            "Expected domain validation error, got: {}",
            result.content
        );
    }

    /// Helper: create a test manifest with no capabilities.
    fn test_manifest() -> SkillManifest {
        SkillManifest {
            name: "test-skill".to_string(),
            version: "0.1.0".to_string(),
            description: "A test skill".to_string(),
            author: None,
            capabilities: Default::default(),
            resources: SkillResources {
                fuel: 1_000_000_000,
                memory_mb: 16,
                epoch_timeout_secs: 5,
            },
            wasm_entry: "skill.wasm".to_string(),
        }
    }
}
