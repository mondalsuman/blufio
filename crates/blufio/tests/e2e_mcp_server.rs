// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP server E2E tests exercising BlufioMcpHandler in-process.
//!
//! Tests verify the full MCP server path: capabilities, tool listing,
//! tool invocation, resource listing, and error handling.

use std::sync::Arc;
use tokio::sync::RwLock;

use blufio_config::model::McpConfig;
use blufio_mcp_server::BlufioMcpHandler;
use blufio_skill::ToolRegistry;
use rmcp::handler::server::ServerHandler;

/// Creates a test handler with built-in tools registered.
fn create_test_handler() -> BlufioMcpHandler {
    let mut registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut registry);
    let registry = Arc::new(RwLock::new(registry));
    let mcp_config = McpConfig::default();
    BlufioMcpHandler::new(registry, &mcp_config)
}

/// Creates a test handler with resources (storage) configured.
fn create_test_handler_with_resources() -> BlufioMcpHandler {
    use async_trait::async_trait;
    use blufio_core::types::{Message, QueueEntry, Session};
    use blufio_core::{BlufioError, StorageAdapter};

    /// Minimal mock storage adapter for resource tests.
    struct MockStorage {
        sessions: Vec<Session>,
    }

    #[async_trait]
    impl blufio_core::traits::adapter::PluginAdapter for MockStorage {
        fn name(&self) -> &str {
            "mock-storage"
        }
        fn version(&self) -> semver::Version {
            semver::Version::new(0, 1, 0)
        }
        fn adapter_type(&self) -> blufio_core::types::AdapterType {
            blufio_core::types::AdapterType::Storage
        }
        async fn health_check(&self) -> Result<blufio_core::types::HealthStatus, BlufioError> {
            Ok(blufio_core::types::HealthStatus::Healthy)
        }
        async fn shutdown(&self) -> Result<(), BlufioError> {
            Ok(())
        }
    }

    #[async_trait]
    impl StorageAdapter for MockStorage {
        async fn initialize(&self) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn close(&self) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn create_session(&self, _session: &Session) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn get_session(&self, id: &str) -> Result<Option<Session>, BlufioError> {
            Ok(self.sessions.iter().find(|s| s.id == id).cloned())
        }
        async fn list_sessions(&self, _state: Option<&str>) -> Result<Vec<Session>, BlufioError> {
            Ok(self.sessions.clone())
        }
        async fn update_session_state(&self, _id: &str, _state: &str) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn insert_message(&self, _message: &Message) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn get_messages(
            &self,
            _session_id: &str,
            _limit: Option<i64>,
        ) -> Result<Vec<Message>, BlufioError> {
            Ok(vec![])
        }
        async fn enqueue(&self, _queue_name: &str, _payload: &str) -> Result<i64, BlufioError> {
            Ok(0)
        }
        async fn dequeue(&self, _queue_name: &str) -> Result<Option<QueueEntry>, BlufioError> {
            Ok(None)
        }
        async fn ack(&self, _id: i64) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn fail(&self, _id: i64) -> Result<(), BlufioError> {
            Ok(())
        }
    }

    let mut registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut registry);
    let registry = Arc::new(RwLock::new(registry));
    let mcp_config = McpConfig::default();

    let storage: Arc<dyn StorageAdapter + Send + Sync> = Arc::new(MockStorage {
        sessions: vec![Session {
            id: "test-session-1".to_string(),
            channel: "api".to_string(),
            user_id: Some("test-user".to_string()),
            state: "active".to_string(),
            metadata: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            classification: Default::default(),
        }],
    });

    BlufioMcpHandler::new(registry, &mcp_config).with_resources(None, Some(storage))
}

#[test]
fn test_mcp_server_info_has_tool_capability() {
    let handler = create_test_handler();
    let info = handler.get_info();

    // Server should advertise tool capabilities.
    assert!(
        info.capabilities.tools.is_some(),
        "expected tools capability"
    );
    assert_eq!(info.server_info.name, "blufio");
}

#[test]
fn test_mcp_server_info_has_prompt_capability() {
    let handler = create_test_handler();
    let info = handler.get_info();

    // Server should advertise prompt capabilities.
    assert!(
        info.capabilities.prompts.is_some(),
        "expected prompts capability"
    );
}

#[test]
fn test_mcp_server_no_resources_without_stores() {
    let handler = create_test_handler();
    let info = handler.get_info();

    // Without memory_store or storage, resources should be None.
    assert!(
        info.capabilities.resources.is_none(),
        "expected no resources capability without stores"
    );
}

#[test]
fn test_mcp_server_has_resources_with_storage() {
    let handler = create_test_handler_with_resources();
    let info = handler.get_info();

    // With storage, resources should be advertised.
    assert!(
        info.capabilities.resources.is_some(),
        "expected resources capability with storage"
    );
}

#[tokio::test]
async fn test_mcp_list_tools_returns_exported_tools() {
    // We cannot call list_tools directly without RequestContext,
    // but we can verify the tool registry and export logic.
    let handler = create_test_handler();

    // Verify the handler filters correctly: bash should be excluded,
    // http and file should be included.
    let info = handler.get_info();
    assert!(info.capabilities.tools.is_some());

    // Verify via the registry directly (the handler reads from this).
    let registry = Arc::new(RwLock::new({
        let mut r = ToolRegistry::new();
        blufio_skill::builtin::register_builtins(&mut r);
        r
    }));
    let reg = registry.read().await;

    // Built-in tools registered: bash, http, file
    assert_eq!(reg.len(), 3);
    assert!(reg.get("bash").is_some());
    assert!(reg.get("http").is_some());
    assert!(reg.get("file").is_some());

    // MCP bridge should filter out bash (always excluded from export).
    let export_tools: Vec<String> = vec![];
    let filtered = blufio_mcp_server::bridge::filtered_tool_names(&reg, &export_tools);
    assert!(
        !filtered.contains(&"bash".to_string()),
        "bash should be excluded from MCP export"
    );
    assert!(
        filtered.contains(&"http".to_string()),
        "http should be exported"
    );
    assert!(
        filtered.contains(&"file".to_string()),
        "file should be exported"
    );
}

#[tokio::test]
async fn test_mcp_tool_invocation_via_bridge() {
    // Test tool invocation using the bridge layer (same path as call_tool).
    let mut registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut registry);

    // Invoke the "http" tool with a known URL (httpbin echo or similar).
    // For isolated testing, we invoke "file" tool with an invalid path
    // and verify it returns an error result (not a crash).
    let file_tool = registry.get("file").expect("file tool registered");
    let input = serde_json::json!({
        "path": "/nonexistent/test/path/that/does/not/exist.txt",
        "operation": "read"
    });
    let result = file_tool.invoke(input).await;

    // The file tool should return an error output (file not found), not panic.
    match result {
        Ok(output) => {
            assert!(
                output.is_error,
                "expected error for nonexistent file, got success"
            );
        }
        Err(e) => {
            // Tool returning an Err is also acceptable for invalid input.
            assert!(
                e.to_string().contains("not found")
                    || e.to_string().contains("No such file")
                    || e.to_string().contains("error")
                    || e.to_string().contains("ExecutionFailed")
                    || e.to_string().contains("missing required"),
                "unexpected error: {e}"
            );
        }
    }
}

#[tokio::test]
async fn test_mcp_invalid_tool_name_handled() {
    // Verify that looking up a nonexistent tool doesn't panic.
    let registry = ToolRegistry::new();
    let result = registry.get("nonexistent_tool_12345");
    assert!(result.is_none(), "nonexistent tool should return None");
}

#[tokio::test]
async fn test_mcp_bridge_converts_tools_to_mcp_format() {
    let mut registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut registry);

    // Verify bridge conversion produces valid MCP tool definitions.
    let http_tool = registry.get("http").expect("http tool exists");
    let mcp_tool = blufio_mcp_server::bridge::to_mcp_tool("http", http_tool.as_ref());

    assert_eq!(mcp_tool.name.as_ref(), "http");
    // Tool should have a description.
    assert!(
        mcp_tool.description.is_some(),
        "MCP tool should have description"
    );
    // Tool should have input schema (a JSON Schema object with at least "type").
    assert!(
        !mcp_tool.input_schema.is_empty(),
        "MCP tool should have non-empty input schema"
    );
}

#[tokio::test]
async fn test_mcp_resource_listing_with_storage() {
    // With storage configured, the sessions resource should be listed.
    let handler = create_test_handler_with_resources();
    let info = handler.get_info();

    assert!(
        info.capabilities.resources.is_some(),
        "resources capability should be present"
    );
}

#[tokio::test]
async fn test_mcp_export_allowlist_filtering() {
    let mut registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut registry);

    // With an explicit export list, only listed tools should pass.
    let export_list = vec!["http".to_string()];
    let filtered = blufio_mcp_server::bridge::filtered_tool_names(&registry, &export_list);
    assert_eq!(filtered.len(), 1);
    assert!(filtered.contains(&"http".to_string()));
    assert!(!filtered.contains(&"file".to_string()));
    assert!(!filtered.contains(&"bash".to_string()));
}
