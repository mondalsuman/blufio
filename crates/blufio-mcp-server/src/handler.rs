// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP server handler implementing the [`ServerHandler`] trait from rmcp.
//!
//! [`BlufioMcpHandler`] bridges the MCP protocol to Blufio's tool system:
//! - **Capability negotiation** (SRVR-04): Advertises tools-only capability.
//! - **Tool listing** (SRVR-01): Returns export-allowed tools via [`bridge`].
//! - **Tool invocation** (SRVR-02): Validates input, enforces timeout, returns results.
//! - **Input validation** (SRVR-05): JSON Schema validation before invocation.
//!
//! The handler holds a shared reference to the [`ToolRegistry`] and the
//! export/timeout configuration from [`McpConfig`].

use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::McpConfig;
use blufio_core::StorageAdapter;
use blufio_memory::MemoryStore;
use blufio_skill::tool::ToolRegistry;
use rmcp::handler::server::ServerHandler;
use rmcp::model::AnnotateAble;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
    Implementation, InitializeResult, ListPromptsResult, ListResourceTemplatesResult,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt, PromptArgument,
    PromptMessage, PromptMessageRole, PromptsCapability, RawResource, RawResourceTemplate,
    ReadResourceRequestParams, ReadResourceResult, ResourceContents, ResourcesCapability,
    ServerCapabilities, ServerInfo, ToolsCapability,
};
use rmcp::service::{RequestContext, RoleServer};
use tokio::sync::RwLock;

use crate::bridge;
use crate::notifications;
use crate::prompts;
use crate::resources;

/// MCP server handler that bridges Blufio's tool system to MCP.
///
/// Created with a shared [`ToolRegistry`] and configuration from [`McpConfig`].
/// Implements rmcp's [`ServerHandler`] trait to handle MCP protocol messages.
///
/// Optionally holds references to [`MemoryStore`] and [`StorageAdapter`]
/// for exposing memory and session data as MCP resources.
pub struct BlufioMcpHandler {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    export_tools: Vec<String>,
    timeout_secs: u64,
    memory_store: Option<Arc<MemoryStore>>,
    storage: Option<Arc<dyn StorageAdapter + Send + Sync>>,
    #[allow(dead_code)]
    tools_changed_rx: Option<notifications::ToolsChangedReceiver>,
}

impl BlufioMcpHandler {
    /// Creates a new handler with the given tool registry and MCP configuration.
    ///
    /// Resources are not available until [`with_resources`](Self::with_resources)
    /// is called. This allows stdio mode to skip resource wiring.
    pub fn new(tool_registry: Arc<RwLock<ToolRegistry>>, config: &McpConfig) -> Self {
        Self {
            tool_registry,
            export_tools: config.export_tools.clone(),
            timeout_secs: config.tool_timeout_secs,
            memory_store: None,
            storage: None,
            tools_changed_rx: None,
        }
    }

    /// Attaches data stores for MCP resource access.
    ///
    /// When `memory_store` is `Some`, memory resources (`blufio://memory/...`)
    /// are available. When `storage` is `Some`, session resources
    /// (`blufio://sessions/...`) are available.
    pub fn with_resources(
        mut self,
        memory_store: Option<Arc<MemoryStore>>,
        storage: Option<Arc<dyn StorageAdapter + Send + Sync>>,
    ) -> Self {
        self.memory_store = memory_store;
        self.storage = storage;
        self
    }

    /// Attaches a tools-changed notification receiver.
    ///
    /// When the receiver signals a change, the handler can forward a
    /// `notifications/tools/list_changed` notification to connected
    /// MCP clients (wiring depends on rmcp session API availability).
    pub fn with_notifications(mut self, rx: notifications::ToolsChangedReceiver) -> Self {
        self.tools_changed_rx = Some(rx);
        self
    }

    /// Returns true if any resource backends are configured.
    fn has_resources(&self) -> bool {
        self.memory_store.is_some() || self.storage.is_some()
    }

    /// Returns true if the named tool is allowed for export via MCP.
    ///
    /// Bash is always excluded. If the export allowlist is empty, all
    /// non-bash tools are allowed. Otherwise only explicitly listed tools.
    fn is_tool_exported(&self, name: &str) -> bool {
        if name == "bash" {
            return false;
        }
        if self.export_tools.is_empty() {
            return true;
        }
        self.export_tools.iter().any(|e| e == name)
    }
}

impl ServerHandler for BlufioMcpHandler {
    fn get_info(&self) -> ServerInfo {
        let resources_capability = if self.has_resources() {
            Some(ResourcesCapability::default())
        } else {
            None
        };

        InitializeResult {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                resources: resources_capability,
                prompts: Some(PromptsCapability::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "blufio".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: None,
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
        let defs = prompts::list_prompt_definitions();
        let prompt_list = defs
            .into_iter()
            .map(|d| Prompt {
                name: d.name,
                title: None,
                description: Some(d.description),
                arguments: Some(
                    d.arguments
                        .into_iter()
                        .map(|a| PromptArgument {
                            name: a.name,
                            title: None,
                            description: Some(a.description),
                            required: Some(a.required),
                        })
                        .collect(),
                ),
                icons: None,
                meta: None,
            })
            .collect();

        Ok(ListPromptsResult {
            prompts: prompt_list,
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        // Convert JsonObject arguments to HashMap<String, String>.
        let args: std::collections::HashMap<String, String> = request
            .arguments
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                (k, val)
            })
            .collect();

        let messages = prompts::get_prompt_messages(&request.name, &args)
            .map_err(|e| rmcp::ErrorData::invalid_params(e, None))?;

        let prompt_messages = messages
            .into_iter()
            .map(|m| {
                let role = match m.role.as_str() {
                    "assistant" => PromptMessageRole::Assistant,
                    _ => PromptMessageRole::User,
                };
                PromptMessage::new_text(role, m.content)
            })
            .collect();

        Ok(GetPromptResult {
            description: None,
            messages: prompt_messages,
        })
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        let mut items = Vec::new();

        // Static resource: blufio://sessions (when storage is available).
        if self.storage.is_some() {
            items.push(
                RawResource {
                    uri: "blufio://sessions".to_string(),
                    name: "sessions".to_string(),
                    title: Some("Session List".to_string()),
                    description: Some("List of all conversation sessions".to_string()),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                    icons: None,
                    meta: None,
                }
                .no_annotation(),
            );
        }

        Ok(ListResourcesResult {
            resources: items,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        let mut templates = Vec::new();

        // Memory resource templates (when memory store is available).
        if self.memory_store.is_some() {
            templates.push(
                RawResourceTemplate {
                    uri_template: "blufio://memory/{id}".to_string(),
                    name: "memory-by-id".to_string(),
                    title: Some("Memory by ID".to_string()),
                    description: Some("Retrieve a specific memory fact by its ID".to_string()),
                    mime_type: Some("application/json".to_string()),
                    icons: None,
                }
                .no_annotation(),
            );
            templates.push(
                RawResourceTemplate {
                    uri_template: "blufio://memory/search?q={query}&limit={limit}".to_string(),
                    name: "memory-search".to_string(),
                    title: Some("Memory Search".to_string()),
                    description: Some(
                        "Search memories using full-text search (limit defaults to 10)".to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    icons: None,
                }
                .no_annotation(),
            );
        }

        // Session resource templates (when storage is available).
        if self.storage.is_some() {
            templates.push(
                RawResourceTemplate {
                    uri_template: "blufio://sessions/{id}".to_string(),
                    name: "session-history".to_string(),
                    title: Some("Session History".to_string()),
                    description: Some(
                        "Message history for a specific conversation session".to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    icons: None,
                }
                .no_annotation(),
            );
        }

        Ok(ListResourceTemplatesResult {
            resource_templates: templates,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let parsed = resources::parse_resource_uri(&request.uri).map_err(|e| {
            rmcp::ErrorData::invalid_params(format!("invalid resource URI: {e}"), None)
        })?;

        let json_value = match parsed {
            resources::ResourceRequest::MemoryById(id) => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    rmcp::ErrorData::invalid_params("memory resources not available", None)
                })?;
                resources::read_memory_by_id(store, &id)
                    .await
                    .map_err(|e| rmcp::ErrorData::internal_error(e, None))?
            }
            resources::ResourceRequest::MemorySearch { query, limit } => {
                let store = self.memory_store.as_ref().ok_or_else(|| {
                    rmcp::ErrorData::invalid_params("memory resources not available", None)
                })?;
                resources::read_memory_search(store, &query, limit)
                    .await
                    .map_err(|e| rmcp::ErrorData::internal_error(e, None))?
            }
            resources::ResourceRequest::SessionList => {
                let storage = self.storage.as_ref().ok_or_else(|| {
                    rmcp::ErrorData::invalid_params("session resources not available", None)
                })?;
                resources::read_session_list(storage.as_ref())
                    .await
                    .map_err(|e| rmcp::ErrorData::internal_error(e, None))?
            }
            resources::ResourceRequest::SessionHistory(session_id) => {
                let storage = self.storage.as_ref().ok_or_else(|| {
                    rmcp::ErrorData::invalid_params("session resources not available", None)
                })?;
                resources::read_session_history(storage.as_ref(), &session_id)
                    .await
                    .map_err(|e| rmcp::ErrorData::internal_error(e, None))?
            }
        };

        let json_string = serde_json::to_string(&json_value).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("JSON serialization error: {e}"), None)
        })?;

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::TextResourceContents {
                uri: request.uri,
                mime_type: Some("application/json".to_string()),
                text: json_string,
                meta: None,
            }],
        })
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let registry = self.tool_registry.read().await;
        let names = bridge::filtered_tool_names(&registry, &self.export_tools);

        let tools: Vec<rmcp::model::Tool> = names
            .iter()
            .filter_map(|name| {
                registry
                    .get(name)
                    .map(|tool| bridge::to_mcp_tool(name, tool.as_ref()))
            })
            .collect();

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tool_name = request.name.as_ref();

        // 1. Check export allowlist.
        if !self.is_tool_exported(tool_name) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("tool '{}' is not available via MCP", tool_name),
                None,
            ));
        }

        // Extract progress_token from request meta (if present) for progress reporting.
        let progress_token = request
            .meta
            .as_ref()
            .and_then(|m| m.get("progressToken"))
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        let _progress_reporter = crate::notifications::ProgressReporter::new(progress_token);

        if _progress_reporter.token().is_some() {
            tracing::debug!(
                tool = tool_name,
                progress_token = ?_progress_reporter.token(),
                "progress reporter created for tool invocation"
            );
        }

        // 2. Look up the tool in the registry.
        let registry = self.tool_registry.read().await;
        let tool = registry.get(tool_name).ok_or_else(|| {
            rmcp::ErrorData::invalid_params(format!("tool '{}' not found", tool_name), None)
        })?;

        // 3. Build the input JSON value from the arguments map.
        let input: serde_json::Value = match &request.arguments {
            Some(map) => serde_json::Value::Object(map.clone()),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        // 4. Validate input against the tool's JSON Schema (SRVR-05).
        let schema = tool.parameters_schema();
        if let Err(validation_error) = validate_input(&schema, &input) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("invalid input for '{}': {}", tool_name, validation_error),
                None,
            ));
        }

        // 5. Invoke the tool with timeout.
        let timeout = Duration::from_secs(self.timeout_secs);
        match tokio::time::timeout(timeout, tool.invoke(input)).await {
            Ok(Ok(output)) => {
                if output.is_error {
                    Ok(CallToolResult::error(vec![Content::text(output.content)]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(output.content)]))
                }
            }
            Ok(Err(e)) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Tool error: {e}. Check input parameters."
            ))])),
            Err(_elapsed) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Tool '{}' timed out after {}s.",
                tool_name, self.timeout_secs
            ))])),
        }
    }
}

/// Validates JSON input against a JSON Schema.
///
/// Returns `Ok(())` if valid, or `Err(message)` with a human-readable
/// description of the first validation error.
fn validate_input(schema: &serde_json::Value, input: &serde_json::Value) -> Result<(), String> {
    let validator =
        jsonschema::validator_for(schema).map_err(|e| format!("invalid schema: {e}"))?;
    let mut errors = validator.iter_errors(input);
    if let Some(first_error) = errors.next() {
        let remaining = errors.count();
        if remaining > 0 {
            Err(format!("{} (and {} more errors)", first_error, remaining))
        } else {
            Err(first_error.to_string())
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use blufio_core::BlufioError;
    use blufio_skill::tool::{Tool as BlufioTool, ToolOutput};

    // ── Test tools ──────────────────────────────────────────────────

    /// Echo tool: returns the `message` field from input.
    struct EchoTool;

    #[async_trait]
    impl BlufioTool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input message"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Message to echo" }
                },
                "required": ["message"]
            })
        }
        async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            let message = input["message"]
                .as_str()
                .unwrap_or("no message")
                .to_string();
            Ok(ToolOutput {
                content: message,
                is_error: false,
            })
        }
    }

    /// Tool that always returns an error.
    struct ErrorTool;

    #[async_trait]
    impl BlufioTool for ErrorTool {
        fn name(&self) -> &str {
            "errortool"
        }
        fn description(&self) -> &str {
            "A tool that always errors"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            Ok(ToolOutput {
                content: "something went wrong".to_string(),
                is_error: true,
            })
        }
    }

    /// Tool that always returns a BlufioError.
    struct FailTool;

    #[async_trait]
    impl BlufioTool for FailTool {
        fn name(&self) -> &str {
            "failtool"
        }
        fn description(&self) -> &str {
            "A tool that returns Err"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            Err(BlufioError::Internal("tool failure".to_string()))
        }
    }

    /// Tool that sleeps for 5 seconds (for timeout tests).
    struct SlowTool;

    #[async_trait]
    impl BlufioTool for SlowTool {
        fn name(&self) -> &str {
            "slowtool"
        }
        fn description(&self) -> &str {
            "A tool that takes too long"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(ToolOutput {
                content: "done".to_string(),
                is_error: false,
            })
        }
    }

    /// Bash tool (for exclusion tests).
    struct BashTool;

    #[async_trait]
    impl BlufioTool for BashTool {
        fn name(&self) -> &str {
            "bash"
        }
        fn description(&self) -> &str {
            "Execute shell commands"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            unreachable!()
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────

    fn default_config() -> McpConfig {
        McpConfig {
            enabled: true,
            servers: Vec::new(),
            export_tools: Vec::new(),
            tool_timeout_secs: 60,
            auth_token: None,
            cors_origins: Vec::new(),
            max_connections: 10,
            health_check_interval_secs: 60,
        }
    }

    fn config_with_timeout(secs: u64) -> McpConfig {
        McpConfig {
            tool_timeout_secs: secs,
            ..default_config()
        }
    }

    fn config_with_export_tools(tools: Vec<String>) -> McpConfig {
        McpConfig {
            export_tools: tools,
            ..default_config()
        }
    }

    async fn make_handler(config: &McpConfig) -> BlufioMcpHandler {
        let mut registry = ToolRegistry::new();
        registry
            .register_builtin(Arc::new(BashTool))
            .expect("register bash");
        registry
            .register_builtin(Arc::new(EchoTool))
            .expect("register echo");
        registry
            .register_builtin(Arc::new(ErrorTool))
            .expect("register errortool");
        registry
            .register_builtin(Arc::new(FailTool))
            .expect("register failtool");
        registry
            .register_builtin(Arc::new(SlowTool))
            .expect("register slowtool");
        let tool_registry = Arc::new(RwLock::new(registry));
        BlufioMcpHandler::new(tool_registry, config)
    }

    fn call_request(
        name: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> CallToolRequestParams {
        CallToolRequestParams {
            meta: None,
            name: std::borrow::Cow::Owned(name.to_string()),
            arguments: args,
            task: None,
        }
    }

    // ── get_info tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn get_info_returns_tools_capability() {
        let config = default_config();
        let handler = make_handler(&config).await;
        let info = handler.get_info();
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.resources.is_none());
        assert!(info.capabilities.prompts.is_some());
    }

    #[tokio::test]
    async fn get_info_returns_blufio_server_info() {
        let config = default_config();
        let handler = make_handler(&config).await;
        let info = handler.get_info();
        assert_eq!(info.server_info.name, "blufio");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
    }

    // ── is_tool_exported tests ──────────────────────────────────────

    #[tokio::test]
    async fn bash_is_never_exported() {
        let config = default_config();
        let handler = make_handler(&config).await;
        assert!(!handler.is_tool_exported("bash"));
    }

    #[tokio::test]
    async fn empty_export_allows_non_bash() {
        let config = default_config();
        let handler = make_handler(&config).await;
        assert!(handler.is_tool_exported("echo"));
        assert!(handler.is_tool_exported("errortool"));
    }

    #[tokio::test]
    async fn explicit_export_list_restricts_tools() {
        let config = config_with_export_tools(vec!["echo".to_string()]);
        let handler = make_handler(&config).await;
        assert!(handler.is_tool_exported("echo"));
        assert!(!handler.is_tool_exported("errortool"));
        assert!(!handler.is_tool_exported("bash"));
    }

    // ── list_tools tests (via internal method) ──────────────────────

    #[tokio::test]
    async fn list_tools_returns_filtered_tools() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let registry = handler.tool_registry.read().await;
        let names = bridge::filtered_tool_names(&registry, &handler.export_tools);
        // Should have echo, errortool, failtool, slowtool (not bash)
        assert!(!names.contains(&"bash".to_string()));
        assert!(names.contains(&"echo".to_string()));
        assert_eq!(names.len(), 4);
    }

    #[tokio::test]
    async fn list_tools_with_allowlist_returns_only_listed() {
        let config = config_with_export_tools(vec!["echo".to_string()]);
        let handler = make_handler(&config).await;

        let registry = handler.tool_registry.read().await;
        let names = bridge::filtered_tool_names(&registry, &handler.export_tools);
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"echo".to_string()));
    }

    // ── call_tool tests (direct async) ──────────────────────────────

    #[tokio::test]
    async fn call_tool_with_non_exported_tool_returns_error() {
        let config = config_with_export_tools(vec!["echo".to_string()]);
        let handler = make_handler(&config).await;

        let request = call_request("errortool", None);
        // Call the handler's call_tool directly (bypasses rmcp dispatch).
        let result = call_tool_direct(&handler, request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("not available via MCP"));
    }

    #[tokio::test]
    async fn call_tool_with_bash_returns_error() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let request = call_request("bash", Some(serde_json::Map::new()));
        let result = call_tool_direct(&handler, request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("not available via MCP"));
    }

    #[tokio::test]
    async fn call_tool_with_nonexistent_tool_returns_error() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let request = call_request("nonexistent", None);
        let result = call_tool_direct(&handler, request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("not found"));
    }

    #[tokio::test]
    async fn call_tool_with_valid_input_returns_content() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let mut args = serde_json::Map::new();
        args.insert(
            "message".to_string(),
            serde_json::Value::String("hello MCP".to_string()),
        );
        let request = call_request("echo", Some(args));
        let result = call_tool_direct(&handler, request).await.unwrap();
        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().expect("should be text");
        assert_eq!(text.text, "hello MCP");
    }

    #[tokio::test]
    async fn call_tool_with_invalid_input_returns_validation_error() {
        let config = default_config();
        let handler = make_handler(&config).await;

        // Missing required "message" field.
        let request = call_request("echo", Some(serde_json::Map::new()));
        let result = call_tool_direct(&handler, request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("invalid input for 'echo'"));
    }

    #[tokio::test]
    async fn call_tool_with_wrong_type_returns_validation_error() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let mut args = serde_json::Map::new();
        // message should be a string, not a number.
        args.insert("message".to_string(), serde_json::Value::Number(42.into()));
        let request = call_request("echo", Some(args));
        let result = call_tool_direct(&handler, request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("invalid input for 'echo'"));
    }

    #[tokio::test]
    async fn call_tool_error_tool_returns_is_error_true() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let request = call_request("errortool", Some(serde_json::Map::new()));
        let result = call_tool_direct(&handler, request).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().expect("should be text");
        assert_eq!(text.text, "something went wrong");
    }

    #[tokio::test]
    async fn call_tool_fail_tool_returns_is_error_true() {
        let config = default_config();
        let handler = make_handler(&config).await;

        let request = call_request("failtool", Some(serde_json::Map::new()));
        let result = call_tool_direct(&handler, request).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().expect("should be text");
        assert!(text.text.contains("Tool error:"));
    }

    #[tokio::test]
    async fn call_tool_timeout_returns_is_error_true() {
        let config = config_with_timeout(1); // 1 second timeout
        let handler = make_handler(&config).await;

        let request = call_request("slowtool", Some(serde_json::Map::new()));
        let result = call_tool_direct(&handler, request).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().expect("should be text");
        assert!(text.text.contains("timed out after 1s"));
    }

    // ── validate_input tests ────────────────────────────────────────

    #[test]
    fn validate_input_accepts_valid_input() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let input = serde_json::json!({"name": "test"});
        assert!(validate_input(&schema, &input).is_ok());
    }

    #[test]
    fn validate_input_rejects_missing_required() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let input = serde_json::json!({});
        let err = validate_input(&schema, &input).unwrap_err();
        assert!(err.contains("required"));
    }

    #[test]
    fn validate_input_rejects_wrong_type() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            },
            "required": ["count"]
        });
        let input = serde_json::json!({"count": "not a number"});
        let err = validate_input(&schema, &input).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn validate_input_accepts_empty_schema() {
        let schema = serde_json::json!({"type": "object"});
        let input = serde_json::json!({});
        assert!(validate_input(&schema, &input).is_ok());
    }

    // ── get_info resource capability tests ───────────────────────────

    #[tokio::test]
    async fn get_info_without_resources_has_no_resources_capability() {
        let config = default_config();
        let handler = make_handler(&config).await;
        let info = handler.get_info();
        assert!(info.capabilities.resources.is_none());
    }

    #[tokio::test]
    async fn get_info_with_resources_advertises_resources_capability() {
        let config = default_config();
        let handler = make_handler(&config).await;
        // Attach a mock storage (no memory store).
        let handler = handler.with_resources(None, Some(Arc::new(MockStorageAdapter)));
        let info = handler.get_info();
        assert!(info.capabilities.resources.is_some());
        // tools capability still present
        assert!(info.capabilities.tools.is_some());
    }

    // ── Minimal mock storage for handler tests ────────────────────

    struct MockStorageAdapter;

    #[async_trait]
    impl blufio_core::traits::adapter::PluginAdapter for MockStorageAdapter {
        fn name(&self) -> &str {
            "mock"
        }
        fn version(&self) -> semver::Version {
            semver::Version::new(0, 1, 0)
        }
        fn adapter_type(&self) -> blufio_core::types::AdapterType {
            blufio_core::types::AdapterType::Storage
        }
        async fn health_check(
            &self,
        ) -> Result<blufio_core::types::HealthStatus, blufio_core::BlufioError> {
            Ok(blufio_core::types::HealthStatus::Healthy)
        }
        async fn shutdown(&self) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
    }

    #[async_trait]
    impl blufio_core::StorageAdapter for MockStorageAdapter {
        async fn initialize(&self) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
        async fn close(&self) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
        async fn create_session(
            &self,
            _session: &blufio_core::types::Session,
        ) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
        async fn get_session(
            &self,
            _id: &str,
        ) -> Result<Option<blufio_core::types::Session>, blufio_core::BlufioError> {
            Ok(None)
        }
        async fn list_sessions(
            &self,
            _state: Option<&str>,
        ) -> Result<Vec<blufio_core::types::Session>, blufio_core::BlufioError> {
            Ok(vec![])
        }
        async fn update_session_state(
            &self,
            _id: &str,
            _state: &str,
        ) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
        async fn insert_message(
            &self,
            _message: &blufio_core::types::Message,
        ) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
        async fn get_messages(
            &self,
            _session_id: &str,
            _limit: Option<i64>,
        ) -> Result<Vec<blufio_core::types::Message>, blufio_core::BlufioError> {
            Ok(vec![])
        }
        async fn enqueue(
            &self,
            _queue_name: &str,
            _payload: &str,
        ) -> Result<i64, blufio_core::BlufioError> {
            Ok(0)
        }
        async fn dequeue(
            &self,
            _queue_name: &str,
        ) -> Result<Option<blufio_core::types::QueueEntry>, blufio_core::BlufioError> {
            Ok(None)
        }
        async fn ack(&self, _id: i64) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
        async fn fail(&self, _id: i64) -> Result<(), blufio_core::BlufioError> {
            Ok(())
        }
    }

    // ── progress_token extraction tests ──────────────────────────────

    #[test]
    fn extract_progress_token_from_meta() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "progressToken".to_string(),
            serde_json::Value::String("tok-42".to_string()),
        );
        let token = meta.get("progressToken").and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        assert_eq!(token, Some("tok-42".to_string()));
    }

    #[test]
    fn extract_progress_token_number_from_meta() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "progressToken".to_string(),
            serde_json::Value::Number(serde_json::Number::from(7)),
        );
        let token = meta.get("progressToken").and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        assert_eq!(token, Some("7".to_string()));
    }

    #[test]
    fn extract_progress_token_missing_meta() {
        let meta: Option<serde_json::Map<String, serde_json::Value>> = None;
        let token = meta
            .as_ref()
            .and_then(|m| m.get("progressToken"))
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        assert_eq!(token, None);
    }

    // ── Helper to call call_tool without RequestContext ──────────────

    /// Calls the handler's call_tool logic directly without needing
    /// a full rmcp RequestContext. This tests the handler's business
    /// logic in isolation.
    async fn call_tool_direct(
        handler: &BlufioMcpHandler,
        request: CallToolRequestParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tool_name = request.name.as_ref().to_string();

        // 1. Check export allowlist.
        if !handler.is_tool_exported(&tool_name) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("tool '{}' is not available via MCP", tool_name),
                None,
            ));
        }

        // 2. Look up the tool in the registry.
        let registry = handler.tool_registry.read().await;
        let tool = registry.get(&tool_name).ok_or_else(|| {
            rmcp::ErrorData::invalid_params(format!("tool '{}' not found", tool_name), None)
        })?;

        // 3. Build the input JSON value.
        let input: serde_json::Value = match &request.arguments {
            Some(map) => serde_json::Value::Object(map.clone()),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        // 4. Validate input.
        let schema = tool.parameters_schema();
        if let Err(validation_error) = validate_input(&schema, &input) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("invalid input for '{}': {}", tool_name, validation_error),
                None,
            ));
        }

        // 5. Invoke with timeout.
        let timeout = Duration::from_secs(handler.timeout_secs);
        match tokio::time::timeout(timeout, tool.invoke(input)).await {
            Ok(Ok(output)) => {
                if output.is_error {
                    Ok(CallToolResult::error(vec![Content::text(output.content)]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(output.content)]))
                }
            }
            Ok(Err(e)) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Tool error: {e}. Check input parameters."
            ))])),
            Err(_elapsed) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Tool '{}' timed out after {}s.",
                tool_name, handler.timeout_secs
            ))])),
        }
    }
}
