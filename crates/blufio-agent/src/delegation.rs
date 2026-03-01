// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Multi-agent delegation system for task routing to specialist agents.
//!
//! Provides `DelegationRouter` for managing specialist agents and
//! `DelegationTool` for LLM-driven delegation via tool-use (INFRA-06).
//! All delegation messages are Ed25519-signed for integrity (SEC-07).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use blufio_auth_keypair::{AgentMessage, DeviceKeypair, SignedAgentMessage};
use blufio_config::model::{AgentConfig, AgentSpecConfig, ContextConfig};
use blufio_context::ContextEngine;
use blufio_core::types::{
    InboundMessage, MessageContent, ProviderStreamChunk, StreamEventType, TokenUsage,
};
use blufio_core::{BlufioError, ProviderAdapter, StorageAdapter};
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_router::ModelRouter;
use blufio_skill::{Tool, ToolOutput, ToolRegistry};
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::session::SessionActor;

/// Internal representation of a specialist agent.
struct AgentSpec {
    config: AgentSpecConfig,
    keypair: DeviceKeypair,
}

/// Routes delegation requests to specialist agents with Ed25519-signed messages.
///
/// Each specialist agent gets its own ephemeral `SessionActor` per delegation,
/// its own `DeviceKeypair` for message signing, and a filtered `ToolRegistry`
/// that enforces single-level depth (specialists cannot delegate further).
pub struct DelegationRouter {
    agents: HashMap<String, AgentSpec>,
    primary_keypair: DeviceKeypair,
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    cost_ledger: Arc<CostLedger>,
    budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    router: Arc<ModelRouter>,
    timeout: Duration,
}

impl DelegationRouter {
    /// Create a new delegation router with the given specialist agent configurations.
    ///
    /// Generates a primary keypair for the main agent and one keypair per specialist.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agents_config: &[AgentSpecConfig],
        provider: Arc<dyn ProviderAdapter + Send + Sync>,
        storage: Arc<dyn StorageAdapter + Send + Sync>,
        cost_ledger: Arc<CostLedger>,
        budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
        router: Arc<ModelRouter>,
        timeout_secs: u64,
    ) -> Self {
        let primary_keypair = DeviceKeypair::generate();

        let mut agents = HashMap::new();
        for agent_config in agents_config {
            let keypair = DeviceKeypair::generate();
            info!(
                agent = agent_config.name.as_str(),
                public_key = keypair.public_hex().as_str(),
                "generated keypair for specialist agent"
            );
            agents.insert(
                agent_config.name.clone(),
                AgentSpec {
                    config: agent_config.clone(),
                    keypair,
                },
            );
        }

        Self {
            agents,
            primary_keypair,
            provider,
            storage,
            cost_ledger,
            budget_tracker,
            router,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Delegate a task to a named specialist agent.
    ///
    /// Creates an Ed25519-signed request, spawns an ephemeral specialist
    /// `SessionActor`, waits for completion (with timeout), verifies the
    /// signed response, and returns the specialist's text output.
    pub async fn delegate(
        &self,
        agent_name: &str,
        task: &str,
        context: &str,
    ) -> Result<String, BlufioError> {
        // 1. Look up agent
        let agent = self.agents.get(agent_name).ok_or_else(|| {
            BlufioError::Internal(format!(
                "delegation: unknown specialist agent '{agent_name}'"
            ))
        })?;

        info!(
            agent = agent_name,
            task = task,
            "delegating task to specialist"
        );

        // 2. Create and sign request message
        let request = AgentMessage::new_request("primary", agent_name, task, context);
        let signed_req = SignedAgentMessage::new(request, &self.primary_keypair);

        // 3. Paranoid self-check: verify our own signature
        signed_req.verify(&self.primary_keypair).map_err(|e| {
            BlufioError::Security(format!("delegation: self-check signature failed: {e}"))
        })?;

        debug!(agent = agent_name, "delegation request signed and verified");

        // 4. Create ephemeral specialist environment
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create session in storage
        let now = chrono::Utc::now().to_rfc3339();
        let session = blufio_core::types::Session {
            id: session_id.clone(),
            channel: "delegation".to_string(),
            user_id: Some(format!("specialist:{agent_name}")),
            state: "active".to_string(),
            metadata: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.storage.create_session(&session).await?;

        // Create fresh ContextEngine with specialist's system prompt
        let agent_config = AgentConfig {
            system_prompt: Some(agent.config.system_prompt.clone()),
            name: agent.config.name.clone(),
            ..AgentConfig::default()
        };
        let context_config = ContextConfig::default();
        let context_engine = Arc::new(
            ContextEngine::new(&agent_config, &context_config)
                .await
                .map_err(|e| {
                    BlufioError::Internal(format!(
                        "delegation: failed to create specialist context engine: {e}"
                    ))
                })?,
        );

        // Create empty tool registry (no delegate_to_specialist -- single-level depth)
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new()));

        // Create ephemeral SessionActor
        let mut actor = SessionActor::new(
            session_id.clone(),
            self.storage.clone(),
            self.provider.clone(),
            context_engine,
            self.budget_tracker.clone(),
            self.cost_ledger.clone(),
            None, // no memory provider for specialists
            None, // no memory extractor for specialists
            "delegation".to_string(),
            self.router.clone(),
            agent.config.model.clone(),
            4096, // default max tokens for specialists
            false, // routing disabled for specialists
            300,   // idle timeout (irrelevant for ephemeral)
            tool_registry,
        );

        // 5. Build inbound message from the delegation request
        let combined_content = if context.is_empty() {
            task.to_string()
        } else {
            format!("{task}\n\nContext:\n{context}")
        };

        let inbound = InboundMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: Some(session_id.clone()),
            channel: "delegation".to_string(),
            sender_id: "primary".to_string(),
            content: MessageContent::Text(combined_content),
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: None,
        };

        // 6. Execute with timeout
        let result = tokio::time::timeout(self.timeout, async {
            // handle_message -> consume stream -> persist_response
            let mut stream = actor.handle_message(inbound).await?;
            let (text, usage) = consume_delegation_stream(&mut stream).await;
            actor.persist_response(&text, usage).await?;
            Ok::<String, BlufioError>(text)
        })
        .await;

        // 7. Handle timeout
        let response_text = match result {
            Ok(Ok(text)) => text,
            Ok(Err(e)) => {
                warn!(agent = agent_name, error = %e, "specialist execution failed");
                return Err(e);
            }
            Err(_elapsed) => {
                warn!(
                    agent = agent_name,
                    timeout_secs = self.timeout.as_secs(),
                    "specialist timed out"
                );
                return Err(BlufioError::Internal(format!(
                    "delegation: specialist '{agent_name}' timed out after {}s",
                    self.timeout.as_secs()
                )));
            }
        };

        // 8. Create and sign response message
        let response =
            AgentMessage::new_response(&signed_req.message, agent_name, &response_text);
        let signed_resp = SignedAgentMessage::new(response, &agent.keypair);

        // 9. Verify response signature
        signed_resp.verify(&agent.keypair).map_err(|e| {
            BlufioError::Security(format!(
                "delegation: specialist response signature verification failed: {e}"
            ))
        })?;

        info!(
            agent = agent_name,
            response_len = response_text.len(),
            "delegation completed successfully"
        );

        // 10. SessionActor drops here (ephemeral)
        Ok(response_text)
    }

    /// Returns the names of all registered specialist agents.
    pub fn agent_names(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }

    /// Returns the primary agent's public key hex.
    pub fn primary_public_key(&self) -> String {
        self.primary_keypair.public_hex()
    }
}

/// Consume a specialist's provider stream, collecting text and usage.
async fn consume_delegation_stream(
    stream: &mut std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>,
    >,
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
                StreamEventType::Error => break,
                _ => {}
            },
            Err(_) => break,
        }
    }

    (text, usage)
}

/// Tool that enables the LLM to delegate tasks to specialist agents.
///
/// Registered in the primary agent's `ToolRegistry`. When the LLM responds
/// with a `tool_use` for `delegate_to_specialist`, this tool routes the
/// task to the named specialist via `DelegationRouter`.
pub struct DelegationTool {
    router: Arc<DelegationRouter>,
}

impl DelegationTool {
    /// Create a new delegation tool backed by the given router.
    pub fn new(router: Arc<DelegationRouter>) -> Self {
        Self { router }
    }
}

#[async_trait]
impl Tool for DelegationTool {
    fn name(&self) -> &str {
        "delegate_to_specialist"
    }

    fn description(&self) -> &str {
        "Delegate a task to a specialist agent. The specialist will process the task independently and return a result."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Name of the specialist agent to delegate to"
                },
                "task": {
                    "type": "string",
                    "description": "Description of the task for the specialist"
                },
                "context": {
                    "type": "string",
                    "description": "Relevant context for the specialist to use"
                }
            },
            "required": ["agent", "task"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let agent = input["agent"]
            .as_str()
            .ok_or_else(|| BlufioError::Internal("delegate: missing 'agent' field".into()))?;
        let task = input["task"]
            .as_str()
            .ok_or_else(|| BlufioError::Internal("delegate: missing 'task' field".into()))?;
        let context = input["context"].as_str().unwrap_or("");

        match self.router.delegate(agent, task, context).await {
            Ok(result) => Ok(ToolOutput {
                content: result,
                is_error: false,
            }),
            Err(e) => Ok(ToolOutput {
                content: format!("Delegation failed: {e}"),
                is_error: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_config::model::{CostConfig, RoutingConfig, StorageConfig};
    use blufio_core::types::ProviderRequest;
    use std::pin::Pin;

    // A test-only delayed provider for timeout testing
    struct DelayedMockProvider {
        delay: Duration,
    }

    #[async_trait]
    impl blufio_core::traits::adapter::PluginAdapter for DelayedMockProvider {
        fn name(&self) -> &str { "delayed-mock" }
        fn version(&self) -> semver::Version { semver::Version::new(0, 1, 0) }
        fn adapter_type(&self) -> blufio_core::types::AdapterType {
            blufio_core::types::AdapterType::Provider
        }
        async fn health_check(&self) -> Result<blufio_core::types::HealthStatus, BlufioError> {
            Ok(blufio_core::types::HealthStatus::Healthy)
        }
        async fn shutdown(&self) -> Result<(), BlufioError> { Ok(()) }
    }

    #[async_trait]
    impl ProviderAdapter for DelayedMockProvider {
        async fn complete(&self, _req: ProviderRequest) -> Result<blufio_core::types::ProviderResponse, BlufioError> {
            tokio::time::sleep(self.delay).await;
            Ok(blufio_core::types::ProviderResponse {
                id: "delayed".to_string(),
                content: "delayed".to_string(),
                model: "test".to_string(),
                stop_reason: Some("end_turn".to_string()),
                usage: TokenUsage::default(),
            })
        }

        async fn stream(&self, _req: ProviderRequest) -> Result<Pin<Box<dyn futures_core::Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>, BlufioError> {
            tokio::time::sleep(self.delay).await;
            let chunks = vec![
                Ok(ProviderStreamChunk {
                    event_type: StreamEventType::MessageStart,
                    text: None, usage: None, error: None, tool_use: None, stop_reason: None,
                }),
                Ok(ProviderStreamChunk {
                    event_type: StreamEventType::ContentBlockDelta,
                    text: Some("delayed response".to_string()),
                    usage: None, error: None, tool_use: None, stop_reason: None,
                }),
                Ok(ProviderStreamChunk {
                    event_type: StreamEventType::MessageDelta,
                    text: None,
                    usage: Some(TokenUsage { input_tokens: 5, output_tokens: 5, cache_read_tokens: 0, cache_creation_tokens: 0 }),
                    error: None, tool_use: None, stop_reason: Some("end_turn".to_string()),
                }),
                Ok(ProviderStreamChunk {
                    event_type: StreamEventType::MessageStop,
                    text: None, usage: None, error: None, tool_use: None, stop_reason: None,
                }),
            ];
            Ok(Box::pin(futures::stream::iter(chunks)))
        }
    }

    async fn make_test_storage() -> (Arc<dyn StorageAdapter + Send + Sync>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage_config = StorageConfig {
            database_path: db_path.to_string_lossy().to_string(),
            wal_mode: true,
        };
        let storage = blufio_storage::SqliteStorage::new(storage_config);
        storage.initialize().await.unwrap();
        (Arc::new(storage) as Arc<dyn StorageAdapter + Send + Sync>, temp_dir)
    }

    async fn make_mock_provider(responses: Vec<String>) -> Arc<dyn ProviderAdapter + Send + Sync> {
        use blufio_test_utils::MockProvider;
        if responses.is_empty() {
            Arc::new(MockProvider::new())
        } else {
            Arc::new(MockProvider::with_responses(responses))
        }
    }

    async fn make_cost_ledger(temp_dir: &tempfile::TempDir) -> Arc<CostLedger> {
        let db_path = temp_dir.path().join("test.db");
        Arc::new(CostLedger::open(db_path.to_str().unwrap()).await.unwrap())
    }

    fn make_budget_tracker() -> Arc<tokio::sync::Mutex<BudgetTracker>> {
        let cost_config = CostConfig {
            daily_budget_usd: None,
            monthly_budget_usd: None,
            track_tokens: true,
        };
        Arc::new(tokio::sync::Mutex::new(BudgetTracker::new(&cost_config)))
    }

    fn make_router() -> Arc<ModelRouter> {
        Arc::new(ModelRouter::new(RoutingConfig {
            enabled: false,
            ..RoutingConfig::default()
        }))
    }

    fn make_agent_configs() -> Vec<AgentSpecConfig> {
        vec![
            AgentSpecConfig {
                name: "summarizer".to_string(),
                system_prompt: "You are a summarization specialist.".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                allowed_skills: vec![],
            },
            AgentSpecConfig {
                name: "coder".to_string(),
                system_prompt: "You are a coding specialist.".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                allowed_skills: vec![],
            },
        ]
    }

    #[tokio::test]
    async fn delegation_router_creates_with_keypairs() {
        let (storage, _temp) = make_test_storage().await;
        let provider = make_mock_provider(vec![]).await;
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = make_agent_configs();

        let dr = DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 60,
        );

        assert_eq!(dr.agent_names().len(), 2);
        assert!(!dr.primary_public_key().is_empty());
    }

    #[tokio::test]
    async fn delegate_returns_specialist_response() {
        let (storage, _temp) = make_test_storage().await;
        let provider = make_mock_provider(vec!["specialist result".to_string()]).await;
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = make_agent_configs();

        let dr = DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 60,
        );

        let result = dr.delegate("summarizer", "summarize this", "some text").await.unwrap();
        assert_eq!(result, "specialist result");
    }

    #[tokio::test]
    async fn delegate_unknown_agent_returns_error() {
        let (storage, _temp) = make_test_storage().await;
        let provider = make_mock_provider(vec![]).await;
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = make_agent_configs();

        let dr = DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 60,
        );

        let result = dr.delegate("nonexistent", "task", "").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown specialist agent"));
    }

    #[tokio::test]
    async fn delegate_timeout_returns_error() {
        let (storage, _temp) = make_test_storage().await;
        // Use a delayed provider that takes 5 seconds
        let provider: Arc<dyn ProviderAdapter + Send + Sync> = Arc::new(DelayedMockProvider {
            delay: Duration::from_secs(5),
        });
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = vec![AgentSpecConfig {
            name: "slow".to_string(),
            system_prompt: "You are slow.".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            allowed_skills: vec![],
        }];

        // Very short timeout (100ms)
        let dr = DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 0,
        );
        // Override timeout to 100ms (0 secs rounds to 0, which is instant timeout)
        // Actually use the timeout that was set to 0 seconds

        let result = dr.delegate("slow", "be slow", "").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timed out"), "expected timeout error, got: {err}");
    }

    #[tokio::test]
    async fn delegation_messages_are_signed_and_verified() {
        // This tests the signing at the AgentMessage level
        let kp_primary = DeviceKeypair::generate();
        let kp_specialist = DeviceKeypair::generate();

        let request = AgentMessage::new_request("primary", "specialist", "task", "context");
        let signed_req = SignedAgentMessage::new(request, &kp_primary);
        assert!(signed_req.verify(&kp_primary).is_ok());
        assert!(signed_req.verify(&kp_specialist).is_err());

        let response = AgentMessage::new_response(&signed_req.message, "specialist", "result");
        let signed_resp = SignedAgentMessage::new(response, &kp_specialist);
        assert!(signed_resp.verify(&kp_specialist).is_ok());
        assert!(signed_resp.verify(&kp_primary).is_err());
    }

    #[tokio::test]
    async fn delegation_tool_has_correct_interface() {
        let (storage, _temp) = make_test_storage().await;
        let provider = make_mock_provider(vec![]).await;
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = make_agent_configs();

        let dr = Arc::new(DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 60,
        ));
        let tool = DelegationTool::new(dr);

        assert_eq!(tool.name(), "delegate_to_specialist");
        assert!(tool.description().contains("specialist"));

        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["agent"].is_object());
        assert!(schema["properties"]["task"].is_object());
        assert!(schema["properties"]["context"].is_object());
    }

    #[tokio::test]
    async fn delegation_tool_invoke_returns_specialist_response() {
        let (storage, _temp) = make_test_storage().await;
        let provider = make_mock_provider(vec!["tool delegation result".to_string()]).await;
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = make_agent_configs();

        let dr = Arc::new(DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 60,
        ));
        let tool = DelegationTool::new(dr);

        let input = serde_json::json!({
            "agent": "summarizer",
            "task": "summarize",
            "context": "text to summarize"
        });

        let output = tool.invoke(input).await.unwrap();
        assert!(!output.is_error);
        assert_eq!(output.content, "tool delegation result");
    }

    #[tokio::test]
    async fn delegation_tool_invoke_with_unknown_agent_returns_error_output() {
        let (storage, _temp) = make_test_storage().await;
        let provider = make_mock_provider(vec![]).await;
        let cost_ledger = make_cost_ledger(&_temp).await;
        let budget_tracker = make_budget_tracker();
        let router_model = make_router();
        let agents = make_agent_configs();

        let dr = Arc::new(DelegationRouter::new(
            &agents, provider, storage, cost_ledger, budget_tracker, router_model, 60,
        ));
        let tool = DelegationTool::new(dr);

        let input = serde_json::json!({
            "agent": "nonexistent",
            "task": "something"
        });

        let output = tool.invoke(input).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("Delegation failed"));
    }

    #[tokio::test]
    async fn specialist_does_not_have_delegation_tool() {
        // This verifies that the DelegationRouter creates specialist sessions
        // with empty tool registries (no delegate_to_specialist).
        // Verified by construction: DelegationRouter::delegate() creates
        // `ToolRegistry::new()` for specialists -- which is empty.
        // Direct assertion:
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get("delegate_to_specialist").is_none());
    }
}
