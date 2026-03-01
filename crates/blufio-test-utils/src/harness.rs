// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test harness for end-to-end integration testing.
//!
//! `TestHarness` assembles a complete agent stack with mock adapters,
//! temp SQLite database, and all required subsystems. Provides
//! `send_message()` to drive the full agent pipeline in tests.

use std::pin::Pin;
use std::sync::Arc;

use blufio_config::model::{
    AgentConfig, BlufioConfig, ContextConfig, CostConfig, RoutingConfig, StorageConfig,
};
use blufio_context::ContextEngine;
use blufio_core::types::{
    InboundMessage, MessageContent, ProviderStreamChunk, StreamEventType, TokenUsage,
};
use blufio_core::{BlufioError, ProviderAdapter, StorageAdapter};
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_router::ModelRouter;
use blufio_skill::ToolRegistry;
use blufio_storage::SqliteStorage;
use futures::{Stream, StreamExt};
use tokio::sync::RwLock;

use crate::mock_channel::MockChannel;
use crate::mock_provider::MockProvider;

/// Builder for creating test environments with configurable options.
pub struct TestHarnessBuilder {
    responses: Vec<String>,
    daily_budget_usd: Option<f64>,
    system_prompt: Option<String>,
}

impl TestHarnessBuilder {
    fn new() -> Self {
        Self {
            responses: Vec::new(),
            daily_budget_usd: None,
            system_prompt: None,
        }
    }

    /// Set mock provider responses.
    pub fn with_mock_responses(mut self, responses: Vec<String>) -> Self {
        self.responses = responses;
        self
    }

    /// Set a daily budget cap for the test environment.
    pub fn with_budget(mut self, daily_usd: f64) -> Self {
        self.daily_budget_usd = Some(daily_usd);
        self
    }

    /// Set a custom system prompt.
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    /// Build the test harness, creating all required subsystems.
    pub async fn build(self) -> Result<TestHarness, BlufioError> {
        // Create temp directory for SQLite
        let temp_dir =
            tempfile::TempDir::new().map_err(|e| BlufioError::Storage { source: e.into() })?;
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_string_lossy().to_string();

        // Initialize SQLite storage
        let storage_config = StorageConfig {
            database_path: db_path_str.clone(),
            wal_mode: true,
        };
        let storage = SqliteStorage::new(storage_config);
        storage.initialize().await?;
        let storage: Arc<dyn StorageAdapter + Send + Sync> = Arc::new(storage);

        // Create cost ledger with same DB
        let cost_ledger = Arc::new(CostLedger::open(&db_path_str).await?);

        // Create budget tracker
        let cost_config = CostConfig {
            daily_budget_usd: self.daily_budget_usd,
            monthly_budget_usd: None,
            track_tokens: true,
        };
        let budget_tracker = Arc::new(tokio::sync::Mutex::new(BudgetTracker::new(&cost_config)));

        // Create context engine with defaults
        let agent_config = AgentConfig {
            system_prompt: self.system_prompt.or(Some("You are a test assistant.".to_string())),
            ..AgentConfig::default()
        };
        let context_config = ContextConfig::default();
        let context_engine = Arc::new(ContextEngine::new(&agent_config, &context_config).await?);

        // Create model router with routing disabled
        let routing_config = RoutingConfig {
            enabled: false,
            ..RoutingConfig::default()
        };
        let router = Arc::new(ModelRouter::new(routing_config.clone()));

        // Create empty tool registry
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new()));

        // Create mock provider
        let mock_provider = Arc::new(if self.responses.is_empty() {
            MockProvider::new()
        } else {
            MockProvider::with_responses(self.responses)
        });

        // Create mock channel
        let mock_channel = Arc::new(MockChannel::new());

        // Build config
        let config = BlufioConfig {
            agent: agent_config,
            context: context_config,
            cost: cost_config,
            routing: routing_config,
            ..BlufioConfig::default()
        };

        Ok(TestHarness {
            mock_provider,
            mock_channel,
            storage,
            cost_ledger,
            budget_tracker,
            context_engine,
            router,
            tool_registry,
            config,
            _temp_dir: temp_dir,
        })
    }
}

/// A complete test environment with mock adapters and temp storage.
///
/// Provides access to all subsystems for assertions and a `send_message()`
/// method that drives the full agent pipeline (storage -> context -> provider -> cost).
pub struct TestHarness {
    /// The mock LLM provider.
    pub mock_provider: Arc<MockProvider>,
    /// The mock channel adapter.
    pub mock_channel: Arc<MockChannel>,
    /// SQLite storage adapter (temp DB, cleaned up on drop).
    pub storage: Arc<dyn StorageAdapter + Send + Sync>,
    /// Cost ledger for recording and querying costs.
    pub cost_ledger: Arc<CostLedger>,
    /// Budget tracker for enforcing spending limits.
    pub budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    /// Context engine for prompt assembly.
    pub context_engine: Arc<ContextEngine>,
    /// Model router (routing disabled by default).
    pub router: Arc<ModelRouter>,
    /// Tool registry (empty by default).
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Blufio configuration.
    pub config: BlufioConfig,
    /// Temp directory kept alive for cleanup on drop.
    _temp_dir: tempfile::TempDir,
}

impl TestHarness {
    /// Create a new builder for configuring the test harness.
    pub fn builder() -> TestHarnessBuilder {
        TestHarnessBuilder::new()
    }

    /// Send a message through the full agent pipeline and return the response text.
    ///
    /// This method:
    /// 1. Creates a session in storage if it does not exist
    /// 2. Creates a SessionActor with all subsystems
    /// 3. Calls `handle_message()` to persist input and get a provider stream
    /// 4. Consumes the stream to collect the response text and usage
    /// 5. Calls `persist_response()` to record the assistant message and costs
    /// 6. Returns the full response text
    pub async fn send_message(&self, text: &str) -> Result<String, BlufioError> {
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create session in storage
        let now = chrono::Utc::now().to_rfc3339();
        let session = blufio_core::types::Session {
            id: session_id.clone(),
            channel: "mock".to_string(),
            user_id: Some("test-user".to_string()),
            state: "active".to_string(),
            metadata: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.storage.create_session(&session).await?;

        // Create a SessionActor
        use blufio_agent::session::SessionActor;
        let mut actor = SessionActor::new(
            session_id.clone(),
            self.storage.clone(),
            self.mock_provider.clone() as Arc<dyn ProviderAdapter + Send + Sync>,
            self.context_engine.clone(),
            self.budget_tracker.clone(),
            self.cost_ledger.clone(),
            None, // no memory provider
            None, // no memory extractor
            "mock".to_string(),
            self.router.clone(),
            self.config.anthropic.default_model.clone(),
            self.config.anthropic.max_tokens,
            self.config.routing.enabled,
            self.config.memory.idle_timeout_secs,
            self.tool_registry.clone(),
        );

        // Create inbound message
        let inbound = InboundMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: Some(session_id.clone()),
            channel: "mock".to_string(),
            sender_id: "test-user".to_string(),
            content: MessageContent::Text(text.to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: None,
        };

        // Handle message (persists user message, assembles context, streams from provider)
        let mut stream = actor.handle_message(inbound).await?;

        // Consume stream
        let (response_text, usage) = consume_stream(&mut stream).await;

        // Persist response (records assistant message and costs)
        actor.persist_response(&response_text, usage).await?;

        Ok(response_text)
    }

    /// Add a response to the mock provider's queue.
    pub async fn add_provider_response(&self, text: String) {
        self.mock_provider.add_response(text).await;
    }
}

/// Consume a provider stream, collecting text and usage.
async fn consume_stream(
    stream: &mut Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
) -> (String, Option<TokenUsage>) {
    let mut text = String::new();
    let mut usage: Option<TokenUsage> = None;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => match chunk.event_type {
                StreamEventType::ContentBlockDelta => {
                    if let Some(t) = &chunk.text {
                        text.push_str(t);
                    }
                }
                StreamEventType::MessageStart | StreamEventType::MessageDelta => {
                    if let Some(u) = chunk.usage {
                        usage = Some(u);
                    }
                }
                StreamEventType::MessageStop => break,
                StreamEventType::Error => {
                    if let Some(err) = &chunk.error {
                        tracing::error!(error = err.as_str(), "stream error in test harness");
                    }
                    break;
                }
                _ => {}
            },
            Err(e) => {
                tracing::error!(error = %e, "stream chunk error in test harness");
                break;
            }
        }
    }

    (text, usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn builder_creates_working_environment() {
        let harness = TestHarness::builder().build().await.unwrap();
        // Storage should be functional
        let sessions = harness.storage.list_sessions(None).await.unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn with_mock_provider_uses_responses() {
        let harness = TestHarness::builder()
            .with_mock_responses(vec!["custom response".to_string()])
            .build()
            .await
            .unwrap();

        let resp = harness.send_message("hello").await.unwrap();
        assert_eq!(resp, "custom response");
    }

    #[tokio::test]
    async fn send_message_returns_mock_response() {
        let harness = TestHarness::builder()
            .with_mock_responses(vec!["test output".to_string()])
            .build()
            .await
            .unwrap();

        let resp = harness.send_message("hello world").await.unwrap();
        assert_eq!(resp, "test output");
    }

    #[tokio::test]
    async fn send_message_persists_messages_in_storage() {
        let harness = TestHarness::builder()
            .with_mock_responses(vec!["stored response".to_string()])
            .build()
            .await
            .unwrap();

        let resp = harness.send_message("store me").await.unwrap();
        assert_eq!(resp, "stored response");

        // Verify messages were persisted
        let sessions = harness.storage.list_sessions(None).await.unwrap();
        assert_eq!(sessions.len(), 1);

        let messages = harness
            .storage
            .get_messages(&sessions[0].id, None)
            .await
            .unwrap();
        // Should have user message + assistant response
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "store me");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "stored response");
    }

    #[tokio::test]
    async fn with_budget_configures_tracker() {
        let harness = TestHarness::builder()
            .with_budget(5.0)
            .build()
            .await
            .unwrap();

        // Budget should be configured
        let mut tracker = harness.budget_tracker.lock().await;
        // Budget check should succeed initially
        assert!(tracker.check_budget().is_ok());
    }

    #[tokio::test]
    async fn cost_is_recorded_after_send_message() {
        let harness = TestHarness::builder()
            .with_mock_responses(vec!["cost tracked".to_string()])
            .build()
            .await
            .unwrap();

        harness.send_message("track my cost").await.unwrap();

        // Verify cost was recorded in the ledger
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let daily_cost = harness.cost_ledger.daily_total(&today).await.unwrap();
        // Should have at least one cost entry (the message cost)
        assert!(
            daily_cost > 0.0,
            "expected non-zero cost, got {daily_cost}"
        );
    }

    #[tokio::test]
    async fn temp_db_is_unique_per_harness() {
        let h1 = TestHarness::builder().build().await.unwrap();
        let h2 = TestHarness::builder().build().await.unwrap();

        // Each should have independent storage
        h1.send_message("msg1").await.ok();
        let s1 = h1.storage.list_sessions(None).await.unwrap();
        let s2 = h2.storage.list_sessions(None).await.unwrap();
        assert_eq!(s1.len(), 1);
        assert_eq!(s2.len(), 0); // h2 has its own DB
    }
}
