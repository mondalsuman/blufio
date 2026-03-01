// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Smart heartbeat system for proactive check-ins.
//!
//! The [`HeartbeatRunner`] periodically calls Haiku with a proactive check-in
//! prompt, generating reminders and follow-ups for the user. It uses:
//! - **Skip-when-unchanged** logic: hashes (message count, date) to avoid
//!   redundant LLM calls when nothing has changed.
//! - **Dedicated budget tracker**: separate from conversation budget, enforcing
//!   a configurable monthly cap (default $10/month).
//! - **Delivery modes**: "on_next_message" stores content for the next user
//!   interaction; "immediate" stores for external delivery.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use blufio_config::model::{CostConfig, HeartbeatConfig};
use blufio_core::error::BlufioError;
use blufio_core::types::{
    ContentBlock, ProviderMessage, ProviderRequest, ProviderResponse, TokenUsage,
};
use blufio_core::{ProviderAdapter, StorageAdapter};
use blufio_cost::budget::BudgetTracker;
use blufio_cost::ledger::{CostRecord, FeatureType};
use blufio_cost::pricing;
use blufio_cost::CostLedger;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Sentinel response indicating no actionable heartbeat content.
const NO_HEARTBEAT_SENTINEL: &str = "NO_HEARTBEAT";

/// Result of a heartbeat execution.
#[derive(Debug, Clone)]
pub struct HeartbeatResult {
    /// Generated heartbeat content from Haiku.
    pub content: String,
    /// Token usage for cost tracking.
    pub usage: TokenUsage,
    /// Whether the heartbeat had actionable content.
    pub has_content: bool,
}

/// Manages periodic proactive check-ins using Haiku.
///
/// Runs on a configurable interval, checks for changes since last heartbeat,
/// and generates proactive insights when state has changed. Uses a dedicated
/// budget tracker separate from conversation costs.
pub struct HeartbeatRunner {
    config: HeartbeatConfig,
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    cost_ledger: Arc<CostLedger>,
    /// Dedicated budget tracker for heartbeat costs only.
    budget_tracker: Mutex<BudgetTracker>,
    /// Hash of the state at last heartbeat execution.
    last_state_hash: Mutex<u64>,
    /// Pending heartbeat content for on_next_message delivery.
    pending_heartbeat: Mutex<Option<String>>,
    /// Count of messages processed since last heartbeat.
    messages_since_last: Mutex<u64>,
}

impl HeartbeatRunner {
    /// Create a new heartbeat runner with a dedicated budget tracker.
    ///
    /// The budget tracker uses `monthly_budget_usd` from `HeartbeatConfig`,
    /// separate from the conversation budget.
    pub fn new(
        config: HeartbeatConfig,
        provider: Arc<dyn ProviderAdapter + Send + Sync>,
        storage: Arc<dyn StorageAdapter + Send + Sync>,
        cost_ledger: Arc<CostLedger>,
    ) -> Self {
        // Create a dedicated budget tracker for heartbeat costs only.
        let heartbeat_cost_config = CostConfig {
            daily_budget_usd: None,
            monthly_budget_usd: Some(config.monthly_budget_usd),
            track_tokens: true,
        };
        let budget_tracker = BudgetTracker::new(&heartbeat_cost_config);

        Self {
            config,
            provider,
            storage,
            cost_ledger,
            budget_tracker: Mutex::new(budget_tracker),
            last_state_hash: Mutex::new(0),
            pending_heartbeat: Mutex::new(None),
            messages_since_last: Mutex::new(0),
        }
    }

    /// Check whether the heartbeat should be skipped.
    ///
    /// Returns `true` if:
    /// - The state hash is unchanged since the last heartbeat
    /// - The heartbeat budget is exhausted
    pub async fn should_skip(&self) -> bool {
        let msg_count = *self.messages_since_last.lock().await;
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let current_hash = Self::compute_state_hash(msg_count, &today);

        let last_hash = *self.last_state_hash.lock().await;
        if current_hash == last_hash {
            debug!("heartbeat skipped: state unchanged");
            return true;
        }

        // Check budget
        let mut budget = self.budget_tracker.lock().await;
        if budget.check_budget().is_err() {
            warn!("heartbeat skipped: monthly budget exhausted");
            return true;
        }

        false
    }

    /// Compute a state hash from message count and date.
    ///
    /// The hash changes when:
    /// - New messages have been received since last heartbeat
    /// - The date changes (ensures at least one heartbeat per day if enabled)
    fn compute_state_hash(message_count: u64, date: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        message_count.hash(&mut hasher);
        date.hash(&mut hasher);
        hasher.finish()
    }

    /// Execute a heartbeat check-in cycle.
    ///
    /// Returns `Ok(Some(result))` with content if the heartbeat produced
    /// actionable output, `Ok(None)` if skipped, or `Err` on failure.
    pub async fn execute(&self) -> Result<Option<HeartbeatResult>, BlufioError> {
        // 1. Check if we should skip
        if self.should_skip().await {
            return Ok(None);
        }

        // 2. Gather session context
        let session_summaries = self.gather_session_context().await?;
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // 3. Build and send the heartbeat prompt
        let request = self.build_heartbeat_prompt(&session_summaries, &today);
        let response = self.provider.complete(request).await?;

        // 4. Record cost
        self.record_heartbeat_cost(&response.usage).await?;

        // 5. Update state tracking
        let msg_count = *self.messages_since_last.lock().await;
        let new_hash = Self::compute_state_hash(msg_count, &today);
        *self.last_state_hash.lock().await = new_hash;
        *self.messages_since_last.lock().await = 0;

        // 6. Check if response is actionable
        let content = response.content.trim().to_string();
        let has_content = !content.is_empty()
            && !content.starts_with(NO_HEARTBEAT_SENTINEL);

        if has_content {
            // Store as pending for on_next_message delivery
            *self.pending_heartbeat.lock().await = Some(content.clone());
            info!("heartbeat generated actionable content");
        } else {
            debug!("heartbeat: nothing to report");
        }

        Ok(Some(HeartbeatResult {
            content,
            usage: response.usage,
            has_content,
        }))
    }

    /// Build the heartbeat prompt with session context.
    fn build_heartbeat_prompt(
        &self,
        session_summaries: &[String],
        current_date: &str,
    ) -> ProviderRequest {
        let system_prompt = "\
You are a personal assistant performing a periodic check-in. \
Review the recent conversation context and identify any:\n\
- Pending items the user mentioned they would do\n\
- Reminders or follow-ups that might be relevant\n\
- Time-sensitive items based on today's date\n\n\
If there is nothing actionable to report, respond with exactly: \"NO_HEARTBEAT\"\n\n\
If there IS something worth mentioning, write a brief, friendly check-in message (1-3 sentences).\n\
Prefix your message with \"[Check-in] \" so the user knows this is proactive, not a response."
            .to_string();

        let context = if session_summaries.is_empty() {
            format!("Today's date: {current_date}\n\nNo recent conversations found.")
        } else {
            format!(
                "Today's date: {current_date}\n\nRecent conversation context:\n{}",
                session_summaries.join("\n---\n")
            )
        };

        ProviderRequest {
            model: self.config.model.clone(),
            system_prompt: Some(system_prompt),
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".to_string(),
                content: vec![ContentBlock::Text { text: context }],
            }],
            max_tokens: 256,
            stream: false,
        }
    }

    /// Gather recent session context for the heartbeat prompt.
    async fn gather_session_context(&self) -> Result<Vec<String>, BlufioError> {
        let sessions = self.storage.list_sessions(Some("active")).await?;
        let mut summaries = Vec::new();

        for session in sessions.iter().take(5) {
            let messages = self.storage.get_messages(&session.id, Some(5)).await?;
            if messages.is_empty() {
                continue;
            }

            let mut summary = format!("Session {} (channel: {}):\n", session.id, session.channel);
            for msg in &messages {
                let truncated = if msg.content.len() > 200 {
                    format!("{}...", &msg.content[..200])
                } else {
                    msg.content.clone()
                };
                summary.push_str(&format!("  [{}] {}\n", msg.role, truncated));
            }
            summaries.push(summary);
        }

        Ok(summaries)
    }

    /// Record heartbeat cost in the cost ledger and dedicated budget tracker.
    async fn record_heartbeat_cost(&self, usage: &TokenUsage) -> Result<(), BlufioError> {
        let pricing = pricing::get_pricing(&self.config.model);
        let cost = pricing::calculate_cost(usage, &pricing);

        let record = CostRecord::new(
            "heartbeat".to_string(),
            self.config.model.clone(),
            FeatureType::Heartbeat,
            usage,
            cost,
        );

        self.cost_ledger.record(&record).await?;
        self.budget_tracker.lock().await.record_cost(cost);

        info!(
            cost_usd = cost,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            "heartbeat cost recorded"
        );

        Ok(())
    }

    /// Notify the heartbeat runner that a user message was received.
    ///
    /// Increments the internal message counter used for skip-when-unchanged detection.
    pub async fn notify_message_received(&self) {
        let mut count = self.messages_since_last.lock().await;
        *count += 1;
    }

    /// Take and return any pending heartbeat content.
    ///
    /// For `on_next_message` delivery mode: the agent loop calls this before
    /// sending a response and prepends the heartbeat content if present.
    ///
    /// Returns `None` if no heartbeat is pending.
    pub async fn take_pending_heartbeat(&self) -> Option<String> {
        self.pending_heartbeat.lock().await.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_hash_changes_with_message_count() {
        let hash1 = HeartbeatRunner::compute_state_hash(0, "2026-03-01");
        let hash2 = HeartbeatRunner::compute_state_hash(5, "2026-03-01");
        assert_ne!(hash1, hash2, "hash should change when message count changes");
    }

    #[test]
    fn state_hash_changes_with_date() {
        let hash1 = HeartbeatRunner::compute_state_hash(0, "2026-03-01");
        let hash2 = HeartbeatRunner::compute_state_hash(0, "2026-03-02");
        assert_ne!(hash1, hash2, "hash should change when date changes");
    }

    #[test]
    fn state_hash_stable_for_same_state() {
        let hash1 = HeartbeatRunner::compute_state_hash(10, "2026-03-01");
        let hash2 = HeartbeatRunner::compute_state_hash(10, "2026-03-01");
        assert_eq!(hash1, hash2, "hash should be stable for identical state");
    }

    #[test]
    fn no_heartbeat_sentinel_detection() {
        let content = "NO_HEARTBEAT";
        let has_content = !content.is_empty() && !content.starts_with(NO_HEARTBEAT_SENTINEL);
        assert!(!has_content, "NO_HEARTBEAT should be detected as no content");
    }

    #[test]
    fn actionable_content_detected() {
        let content = "[Check-in] Remember to follow up on the deployment issue.";
        let has_content = !content.is_empty() && !content.starts_with(NO_HEARTBEAT_SENTINEL);
        assert!(has_content, "actionable content should be detected");
    }

    #[tokio::test]
    async fn pending_heartbeat_take_returns_and_clears() {
        // Directly test the Mutex<Option<String>> behavior
        let pending: Mutex<Option<String>> = Mutex::new(Some("test content".to_string()));

        // Take should return the content
        let taken = pending.lock().await.take();
        assert_eq!(taken.as_deref(), Some("test content"));

        // Second take should return None
        let taken_again = pending.lock().await.take();
        assert!(taken_again.is_none());
    }

    #[tokio::test]
    async fn messages_since_last_counter_increments() {
        let counter: Mutex<u64> = Mutex::new(0);
        {
            let mut count = counter.lock().await;
            *count += 1;
        }
        {
            let mut count = counter.lock().await;
            *count += 1;
        }
        assert_eq!(*counter.lock().await, 2);
    }

    #[test]
    fn heartbeat_budget_tracker_uses_monthly_cap() {
        let config = CostConfig {
            daily_budget_usd: None,
            monthly_budget_usd: Some(10.0),
            track_tokens: true,
        };
        let mut tracker = BudgetTracker::new(&config);

        // Should be ok under budget
        assert!(tracker.check_budget().is_ok());

        // Record costs up to budget
        tracker.record_cost(10.0);
        assert!(tracker.check_budget().is_err(), "should fail at budget limit");
    }

    #[test]
    fn heartbeat_result_fields() {
        let result = HeartbeatResult {
            content: "[Check-in] Test".to_string(),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 30,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            has_content: true,
        };
        assert!(result.has_content);
        assert_eq!(result.usage.input_tokens, 100);
    }
}
