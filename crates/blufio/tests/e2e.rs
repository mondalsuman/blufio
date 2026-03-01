// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end integration tests for the complete Blufio pipeline.
//!
//! Each test creates an isolated TestHarness with temp SQLite, mock adapters,
//! and all required subsystems. Tests are independent and order-insensitive.

use blufio_auth_keypair::{AgentMessage, AgentMessageType, DeviceKeypair, SignedAgentMessage};
use blufio_core::StorageAdapter;
use blufio_test_utils::TestHarness;

// ---- Test 1: Message-to-response pipeline ----

#[tokio::test]
async fn test_message_pipeline_returns_mock_response() {
    let harness = TestHarness::builder()
        .with_mock_responses(vec!["Hello from Blufio!".to_string()])
        .build()
        .await
        .unwrap();

    let response = harness.send_message("Hi there").await.unwrap();
    assert_eq!(response, "Hello from Blufio!");
}

#[tokio::test]
async fn test_message_pipeline_persists_user_and_assistant_messages() {
    let harness = TestHarness::builder()
        .with_mock_responses(vec!["Persisted response".to_string()])
        .build()
        .await
        .unwrap();

    harness.send_message("Test persistence").await.unwrap();

    // Verify session was created
    let sessions = harness.storage.list_sessions(None).await.unwrap();
    assert_eq!(sessions.len(), 1);

    // Verify messages were persisted
    let messages = harness
        .storage
        .get_messages(&sessions[0].id, None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "Test persistence");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].content, "Persisted response");
}

// ---- Test 2: Conversation persistence ----

#[tokio::test]
async fn test_multiple_messages_in_same_harness() {
    let harness = TestHarness::builder()
        .with_mock_responses(vec![
            "First response".to_string(),
            "Second response".to_string(),
        ])
        .build()
        .await
        .unwrap();

    let r1 = harness.send_message("Message 1").await.unwrap();
    let r2 = harness.send_message("Message 2").await.unwrap();

    assert_eq!(r1, "First response");
    assert_eq!(r2, "Second response");

    // Each send_message creates a new session, so we should have 2 sessions
    let sessions = harness.storage.list_sessions(None).await.unwrap();
    assert_eq!(sessions.len(), 2);
}

// ---- Test 3: Cost tracking and budget enforcement ----

#[tokio::test]
async fn test_cost_tracking_records_after_message() {
    let harness = TestHarness::builder()
        .with_mock_responses(vec!["cost test".to_string()])
        .build()
        .await
        .unwrap();

    harness.send_message("track cost").await.unwrap();

    // Verify cost was recorded
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let daily_cost = harness.cost_ledger.daily_total(&today).await.unwrap();
    assert!(
        daily_cost > 0.0,
        "expected non-zero daily cost, got {daily_cost}"
    );
}

#[tokio::test]
async fn test_budget_enforcement_blocks_when_exhausted() {
    let harness = TestHarness::builder()
        .with_mock_responses(vec![
            "resp1".to_string(),
            "resp2".to_string(),
            "resp3".to_string(),
            "resp4".to_string(),
            "resp5".to_string(),
        ])
        // Very small budget -- should be exhausted after a few messages
        .with_budget(0.000001)
        .build()
        .await
        .unwrap();

    // First message should succeed
    let r1 = harness.send_message("first").await;
    assert!(r1.is_ok(), "first message should succeed");

    // Subsequent messages may trigger budget exhaustion
    let mut budget_hit = false;
    for i in 2..=5 {
        let result = harness
            .send_message(&format!("message {i}"))
            .await;
        if result.is_err() {
            let err = result.unwrap_err().to_string();
            if err.contains("budget") || err.contains("Budget") {
                budget_hit = true;
                break;
            }
        }
    }

    assert!(
        budget_hit,
        "expected budget exhaustion error after multiple messages with tiny budget"
    );
}

// ---- Test 4: Ed25519 signing and verification ----

#[tokio::test]
async fn test_ed25519_sign_verify_roundtrip() {
    let primary_kp = DeviceKeypair::generate();

    // Create and sign request with primary keypair
    let request = AgentMessage::new_request("primary", "specialist", "task", "context data");
    assert_eq!(request.message_type, AgentMessageType::Request);

    let signed_req = SignedAgentMessage::new(request, &primary_kp);
    assert!(signed_req.verify(&primary_kp).is_ok(), "should verify with correct keypair");
}

#[tokio::test]
async fn test_ed25519_tampered_message_fails_verification() {
    let kp = DeviceKeypair::generate();

    let msg = AgentMessage::new_request("sender", "recipient", "task", "original");
    let mut signed = SignedAgentMessage::new(msg, &kp);

    // Tamper with content
    signed.message.content = "tampered".to_string();
    signed.signed_bytes = signed.message.canonical_bytes();

    let result = signed.verify(&kp);
    assert!(result.is_err(), "tampered message should fail verification");
}

#[tokio::test]
async fn test_ed25519_wrong_keypair_fails_verification() {
    let kp1 = DeviceKeypair::generate();
    let kp2 = DeviceKeypair::generate();

    let msg = AgentMessage::new_request("sender", "recipient", "task", "content");
    let signed = SignedAgentMessage::new(msg, &kp1);

    let result = signed.verify(&kp2);
    assert!(result.is_err(), "wrong keypair should fail verification");
}

#[tokio::test]
async fn test_ed25519_response_roundtrip() {
    let primary_kp = DeviceKeypair::generate();
    let specialist_kp = DeviceKeypair::generate();

    let request = AgentMessage::new_request("primary", "specialist", "summarize", "data");
    let signed_req = SignedAgentMessage::new(request, &primary_kp);

    // Create and sign response with specialist keypair
    let response = AgentMessage::new_response(&signed_req.message, "specialist", "summary result");
    assert_eq!(response.message_type, AgentMessageType::Response);
    assert_eq!(response.recipient, "primary");

    let signed_resp = SignedAgentMessage::new(response, &specialist_kp);
    assert!(signed_resp.verify(&specialist_kp).is_ok());
    assert!(signed_resp.verify(&primary_kp).is_err());
}

// ---- Test 5: Multi-agent delegation (DelegationRouter unit test) ----

#[tokio::test]
async fn test_delegation_router_delegates_to_specialist() {
    use blufio_agent::DelegationRouter;
    use blufio_config::model::{AgentSpecConfig, CostConfig, RoutingConfig, StorageConfig};
    use blufio_cost::{BudgetTracker, CostLedger};
    use blufio_router::ModelRouter;
    use blufio_storage::SqliteStorage;
    use blufio_test_utils::MockProvider;
    use std::sync::Arc;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("delegation_test.db");
    let db_path_str = db_path.to_string_lossy().to_string();

    // Setup storage
    let storage_config = StorageConfig {
        database_path: db_path_str.clone(),
        wal_mode: true,
    };
    let storage = SqliteStorage::new(storage_config);
    storage.initialize().await.unwrap();
    let storage: Arc<dyn blufio_core::StorageAdapter + Send + Sync> = Arc::new(storage);

    // Setup mock provider
    let provider: Arc<dyn blufio_core::ProviderAdapter + Send + Sync> =
        Arc::new(MockProvider::with_responses(vec![
            "specialist analysis complete".to_string(),
        ]));

    // Setup cost
    let cost_ledger = Arc::new(CostLedger::open(&db_path_str).await.unwrap());
    let cost_config = CostConfig {
        daily_budget_usd: None,
        monthly_budget_usd: None,
        track_tokens: true,
    };
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(BudgetTracker::new(&cost_config)));

    // Setup router
    let router = Arc::new(ModelRouter::new(RoutingConfig {
        enabled: false,
        ..RoutingConfig::default()
    }));

    // Setup agents
    let agents = vec![AgentSpecConfig {
        name: "analyzer".to_string(),
        system_prompt: "You are an analysis specialist.".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        allowed_skills: vec![],
    }];

    let delegation_router = DelegationRouter::new(
        &agents, provider, storage, cost_ledger, budget_tracker, router, 60,
    );

    let result = delegation_router
        .delegate("analyzer", "analyze this data", "some raw data")
        .await
        .unwrap();

    assert_eq!(result, "specialist analysis complete");
}

// ---- Test 6: Default response when no queued responses ----

#[tokio::test]
async fn test_default_mock_response() {
    let harness = TestHarness::builder().build().await.unwrap();

    let response = harness.send_message("anything").await.unwrap();
    assert_eq!(response, "mock response");
}

// ---- Test 7: Independent test isolation ----

#[tokio::test]
async fn test_harness_isolation() {
    // Two harnesses should be completely independent
    let h1 = TestHarness::builder()
        .with_mock_responses(vec!["h1-response".to_string()])
        .build()
        .await
        .unwrap();

    let h2 = TestHarness::builder()
        .with_mock_responses(vec!["h2-response".to_string()])
        .build()
        .await
        .unwrap();

    let r1 = h1.send_message("msg").await.unwrap();
    let r2 = h2.send_message("msg").await.unwrap();

    assert_eq!(r1, "h1-response");
    assert_eq!(r2, "h2-response");

    // Verify independent storage
    let s1 = h1.storage.list_sessions(None).await.unwrap();
    let s2 = h2.storage.list_sessions(None).await.unwrap();
    assert_eq!(s1.len(), 1);
    assert_eq!(s2.len(), 1);
    assert_ne!(s1[0].id, s2[0].id);
}
