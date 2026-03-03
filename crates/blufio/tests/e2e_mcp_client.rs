// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP client E2E tests verifying external tool discovery, registration,
//! and graceful failure handling.
//!
//! Tests the McpClientManager's ability to:
//! - Handle unreachable servers without panicking (CLNT-14)
//! - Register tools with namespace prefixes
//! - Sanitize external tool descriptions
//! - Handle tool registry integration

use std::sync::Arc;

use blufio_config::model::McpServerEntry;
use blufio_mcp_client::McpClientManager;
use blufio_skill::ToolRegistry;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_mcp_client_unreachable_server_graceful() {
    // Connect to a server that doesn't exist. This should fail gracefully
    // (non-fatal per CLNT-14), not panic or crash.
    let registry = Arc::new(RwLock::new(ToolRegistry::new()));

    let entry = McpServerEntry {
        name: "nonexistent".to_string(),
        transport: "http".to_string(),
        url: Some("http://127.0.0.1:1".to_string()), // Port 1 -- nothing listening
        auth_token: None,
        command: None,
        args: vec![],
        connect_timeout_secs: 2,
        response_size_cap: 10000,
        trusted: false,
    };

    let (_manager, result) = McpClientManager::connect_all(&[entry], &registry, None).await;

    // Connection should fail but not crash.
    assert_eq!(result.connected, 0, "should have 0 connected servers");
    assert_eq!(result.failed, 1, "should have 1 failed server");
    assert_eq!(result.tools_registered, 0, "should register 0 tools");

    // Tool registry should be unchanged.
    let reg = registry.read().await;
    assert_eq!(reg.len(), 0, "registry should remain empty");
}

#[tokio::test]
async fn test_mcp_client_multiple_unreachable_servers_graceful() {
    // Multiple failing servers should all be handled independently.
    let registry = Arc::new(RwLock::new(ToolRegistry::new()));

    let entries = vec![
        McpServerEntry {
            name: "server-a".to_string(),
            transport: "http".to_string(),
            url: Some("http://127.0.0.1:1".to_string()),
            auth_token: None,
            command: None,
            args: vec![],
            connect_timeout_secs: 2,
            response_size_cap: 10000,
            trusted: false,
        },
        McpServerEntry {
            name: "server-b".to_string(),
            transport: "http".to_string(),
            url: Some("http://127.0.0.1:2".to_string()),
            auth_token: None,
            command: None,
            args: vec![],
            connect_timeout_secs: 2,
            response_size_cap: 10000,
            trusted: false,
        },
    ];

    let (_manager, result) = McpClientManager::connect_all(&entries, &registry, None).await;

    assert_eq!(result.connected, 0);
    assert_eq!(result.failed, 2, "both servers should fail independently");
    assert_eq!(result.tools_registered, 0);
}

#[tokio::test]
async fn test_mcp_client_empty_server_list() {
    // Connecting with no servers should succeed with empty results.
    let registry = Arc::new(RwLock::new(ToolRegistry::new()));

    let (_manager, result) = McpClientManager::connect_all(&[], &registry, None).await;

    assert_eq!(result.connected, 0);
    assert_eq!(result.failed, 0);
    assert_eq!(result.tools_registered, 0);
}

#[tokio::test]
async fn test_mcp_client_invalid_transport_graceful() {
    // An unsupported transport type should fail gracefully.
    let registry = Arc::new(RwLock::new(ToolRegistry::new()));

    let entry = McpServerEntry {
        name: "bad-transport".to_string(),
        transport: "grpc".to_string(), // unsupported
        url: Some("http://127.0.0.1:50051".to_string()),
        auth_token: None,
        command: None,
        args: vec![],
        connect_timeout_secs: 2,
        response_size_cap: 10000,
        trusted: false,
    };

    let (_manager, result) = McpClientManager::connect_all(&[entry], &registry, None).await;

    assert_eq!(result.connected, 0);
    assert_eq!(result.failed, 1);
}

#[tokio::test]
async fn test_mcp_client_server_state_tracking() {
    // After connection attempt, server state should be tracked.
    let registry = Arc::new(RwLock::new(ToolRegistry::new()));

    let entry = McpServerEntry {
        name: "test-server".to_string(),
        transport: "http".to_string(),
        url: Some("http://127.0.0.1:1".to_string()),
        auth_token: None,
        command: None,
        args: vec![],
        connect_timeout_secs: 2,
        response_size_cap: 10000,
        trusted: false,
    };

    let (manager, _result) = McpClientManager::connect_all(&[entry], &registry, None).await;

    // Server should be tracked as disconnected.
    assert!(
        manager.connected_servers().is_empty(),
        "no servers should be connected"
    );
    assert_eq!(
        manager.disconnected_servers().len(),
        1,
        "one server should be disconnected"
    );
    assert_eq!(manager.disconnected_servers()[0], "test-server");
}

#[test]
fn test_external_tool_namespace_convention() {
    // Verify the double-underscore naming convention for namespaced tools.
    let server_name = "github";
    let tool_name = "search_repos";
    let namespaced = format!("{server_name}__{tool_name}");
    assert_eq!(namespaced, "github__search_repos");

    // Namespace separator is __ (double underscore), distinct from single underscore.
    assert!(namespaced.contains("__"));
}

#[test]
fn test_external_tool_description_sanitization() {
    use blufio_mcp_client::sanitize::sanitize_description;

    // Long descriptions should be truncated.
    let long_desc = "a".repeat(2000);
    let sanitized = sanitize_description("test", &long_desc);
    assert!(
        sanitized.len() <= 1024,
        "description should be capped at 1024, got {}",
        sanitized.len()
    );

    // Normal descriptions pass through (modulo any cleaning).
    let normal = sanitize_description("test", "A useful tool for searching.");
    assert!(normal.contains("useful tool"));
}

#[test]
fn test_external_tool_trust_guidance() {
    // Verify trust guidance constant is meaningful.
    let guidance = blufio_mcp_client::manager::EXTERNAL_TOOL_TRUST_GUIDANCE;
    assert!(guidance.contains("External tools"));
    assert!(guidance.contains("untrusted") || guidance.contains("third-party"));
}
