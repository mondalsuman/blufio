// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dynamic zone: assembles conversation history with dual soft/hard triggers
//! and cascade compaction (L1 then L2 if needed).

use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, CompactionEvent, new_event_id, now_timestamp};
use blufio_config::model::ContextConfig;
use blufio_core::error::BlufioError;
use blufio_core::token_counter::{TokenizerCache, count_with_fallback};
use blufio_core::traits::{ProviderAdapter, StorageAdapter};
use blufio_core::types::{
    ContentBlock, InboundMessage, MessageContent, ProviderMessage, TokenUsage,
};
use tracing::{debug, info, warn};

use crate::compaction::extract::{ExtractionOutput, extract_entities};
use crate::compaction::levels::{CompactionLevel, compact_to_l1, compact_to_l2};
use crate::compaction::persist_compaction_summary_with_level;
use crate::compaction::quality::{GateResult, QualityWeights, evaluate_and_gate};

/// Result of dynamic zone assembly, carrying messages and compaction costs.
#[derive(Debug)]
pub struct DynamicResult {
    /// Assembled messages for the provider request.
    pub messages: Vec<ProviderMessage>,
    /// Token usages from compaction LLM calls (may include multiple if cascade).
    /// Callers MUST propagate each for cost recording with FeatureType::Compaction.
    pub compaction_usages: Vec<TokenUsage>,
    /// Entities extracted before L1 compaction (caller persists as Memory entries).
    pub extracted_entities: Vec<String>,
}

/// Internal quality scoring outcome for compaction gate logic.
enum QualityOutcome {
    /// Quality score passed the proceed threshold.
    Proceed(f64),
    /// Quality score in retry range; contains the weakest dimension name.
    RetryNeeded(String),
    /// Quality score below retry threshold.
    Abort,
    /// Scoring call failed (continue without score).
    ScoringFailed,
}

/// The dynamic zone manages conversation history assembly with
/// dual soft/hard trigger compaction and cascade (L1 then L2).
pub struct DynamicZone {
    /// Whether compaction is enabled.
    compaction_enabled: bool,
    /// Fraction of context budget at which soft compaction triggers (L0->L1).
    soft_trigger: f64,
    /// Fraction of context budget at which hard compaction cascades (L1->L2).
    hard_trigger: f64,
    /// Context window budget in tokens (from config; adaptive budget passed per-call).
    #[allow(dead_code)]
    context_budget: u32,
    /// Model to use for compaction summarization.
    compaction_model: String,
    /// Maximum tokens for L1 compaction (per turn-pair).
    max_tokens_l1: u32,
    /// Maximum tokens for L2 compaction.
    max_tokens_l2: u32,
    /// Cached tokenizer instances for accurate token counting.
    token_cache: Arc<TokenizerCache>,
    /// Optional event bus for compaction lifecycle events.
    event_bus: Option<Arc<EventBus>>,
    /// Whether quality scoring is enabled.
    quality_scoring: bool,
    /// Quality gate proceed threshold (default 0.6).
    quality_gate_proceed: f64,
    /// Quality gate retry threshold (default 0.4).
    quality_gate_retry: f64,
    /// Quality weights for scoring dimensions.
    quality_weights: QualityWeights,
}

impl DynamicZone {
    /// Creates a new dynamic zone from context configuration.
    pub fn new(config: &ContextConfig, token_cache: Arc<TokenizerCache>) -> Self {
        Self {
            compaction_enabled: config.compaction_enabled,
            soft_trigger: config.effective_soft_trigger(),
            hard_trigger: config.hard_trigger,
            context_budget: config.context_budget,
            compaction_model: config.compaction_model.clone(),
            max_tokens_l1: config.max_tokens_l1,
            max_tokens_l2: config.max_tokens_l2,
            token_cache,
            event_bus: None,
            quality_scoring: config.quality_scoring,
            quality_gate_proceed: config.quality_gate_proceed,
            quality_gate_retry: config.quality_gate_retry,
            quality_weights: QualityWeights::from_config(config),
        }
    }

    /// Creates a new dynamic zone with an event bus for compaction events.
    pub fn with_event_bus(
        config: &ContextConfig,
        token_cache: Arc<TokenizerCache>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        let mut zone = Self::new(config, token_cache);
        zone.event_bus = Some(event_bus);
        zone
    }

    /// Assembles conversation messages from storage, triggering compaction if needed.
    ///
    /// Implements dual soft/hard trigger logic:
    /// - Soft trigger (default 50%): fires L0->L1 compaction (turn-pair bullets)
    /// - Hard trigger (default 85%): cascades L1->L2 (session narrative) if L1 insufficient
    ///
    /// Entity extraction runs before L1 compaction. On ANY compaction error,
    /// falls back to truncation of oldest messages (never blocks the agent loop).
    pub async fn assemble_messages(
        &self,
        provider: &dyn ProviderAdapter,
        storage: &dyn StorageAdapter,
        session_id: &str,
        inbound: &InboundMessage,
        model: &str,
        dynamic_budget: u32,
    ) -> Result<DynamicResult, BlufioError> {
        // Load ALL messages from storage for this session.
        let history = storage.get_messages(session_id, None).await?;

        // Defense-in-depth: filter Restricted messages that may have bypassed SQL filter.
        let guard = blufio_security::ClassificationGuard::instance();
        let history: Vec<_> = history
            .into_iter()
            .filter(|msg| {
                if !guard.can_include_in_context(msg.classification) {
                    tracing::info!(
                        message_id = %msg.id,
                        classification = %msg.classification.as_str(),
                        "restricted message excluded from context (defense-in-depth)"
                    );
                    false
                } else {
                    true
                }
            })
            .collect();

        // Accurate token counting via provider-specific tokenizer.
        let counter = self.token_cache.get_counter(model);
        let mut estimated_tokens: usize = 0;
        for m in &history {
            estimated_tokens += count_with_fallback(counter.as_ref(), &m.content).await;
        }

        // Compute thresholds from adaptive dynamic budget.
        // The dynamic_budget is computed by ContextEngine: total - actual_static - actual_conditional.
        // Soft/hard triggers apply to this adaptive budget, not the total context budget.
        let budget = dynamic_budget as usize;
        let soft_threshold = (budget as f64 * self.soft_trigger) as usize;
        let hard_threshold = (budget as f64 * self.hard_trigger) as usize;

        debug!(
            estimated_tokens = estimated_tokens,
            soft_threshold = soft_threshold,
            hard_threshold = hard_threshold,
            history_len = history.len(),
            "dynamic zone token estimate"
        );

        // Decision: no compaction needed, compaction disabled, or too few messages.
        if !self.compaction_enabled || estimated_tokens <= soft_threshold || history.len() <= 2 {
            let msgs: Vec<ProviderMessage> = history
                .iter()
                .map(|msg| ProviderMessage {
                    role: msg.role.clone(),
                    content: vec![ContentBlock::Text {
                        text: msg.content.clone(),
                    }],
                })
                .collect();

            let mut messages = msgs;
            let inbound_content = message_content_to_blocks(&inbound.content);
            messages.push(ProviderMessage {
                role: "user".to_string(),
                content: inbound_content,
            });

            return Ok(DynamicResult {
                messages,
                compaction_usages: vec![],
                extracted_entities: vec![],
            });
        }

        // --- Soft trigger exceeded: fire L0->L1 compaction ---
        let split_point = history.len() / 2;
        let older = &history[..split_point];
        let recent = &history[split_point..];

        info!(
            older_count = older.len(),
            recent_count = recent.len(),
            estimated_tokens = estimated_tokens,
            soft_threshold = soft_threshold,
            hard_threshold = hard_threshold,
            "soft trigger exceeded, starting L1 compaction"
        );

        let mut compaction_usages = Vec::new();
        let mut extracted_entities = Vec::new();

        // Attempt L1 compaction with error fallback.
        let l1_result = self
            .try_l1_compaction(
                provider,
                storage,
                session_id,
                older,
                recent,
                model,
                &mut compaction_usages,
                &mut extracted_entities,
            )
            .await;

        match l1_result {
            Ok((mut msgs, l1_summary_text)) => {
                // Re-estimate tokens after L1 compaction.
                let mut new_estimate: usize = 0;
                for m in &msgs {
                    for block in &m.content {
                        if let ContentBlock::Text { text } = block {
                            new_estimate += count_with_fallback(counter.as_ref(), text).await;
                        }
                    }
                }

                debug!(
                    new_estimate = new_estimate,
                    hard_threshold = hard_threshold,
                    "post-L1 token estimate"
                );

                // --- Hard trigger: cascade to L2 if L1 insufficient ---
                if new_estimate > hard_threshold {
                    info!(
                        new_estimate = new_estimate,
                        hard_threshold = hard_threshold,
                        "hard trigger exceeded, cascading to L2"
                    );

                    match self
                        .try_l2_cascade(
                            provider,
                            storage,
                            session_id,
                            &l1_summary_text,
                            &mut compaction_usages,
                        )
                        .await
                    {
                        Ok(l2_msgs) => {
                            msgs = l2_msgs;
                            // Re-add recent messages after L2 summary.
                            for msg in recent {
                                msgs.push(ProviderMessage {
                                    role: msg.role.clone(),
                                    content: vec![ContentBlock::Text {
                                        text: msg.content.clone(),
                                    }],
                                });
                            }
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "L2 cascade failed, continuing with L1 summary"
                            );
                            // L1 summary is already in msgs, continue with it.
                        }
                    }
                }

                // Append the current inbound message.
                let inbound_content = message_content_to_blocks(&inbound.content);
                msgs.push(ProviderMessage {
                    role: "user".to_string(),
                    content: inbound_content,
                });

                Ok(DynamicResult {
                    messages: msgs,
                    compaction_usages,
                    extracted_entities,
                })
            }
            Err(e) => {
                // Compaction failed entirely: fall back to truncation.
                warn!(
                    error = %e,
                    "compaction failed, falling back to truncation"
                );
                let msgs = self
                    .truncate_to_budget(&history, soft_threshold, counter.as_ref(), inbound)
                    .await;

                Ok(DynamicResult {
                    messages: msgs,
                    compaction_usages,
                    extracted_entities,
                })
            }
        }
    }

    /// Attempts L1 compaction with entity extraction, quality scoring, and event emission.
    ///
    /// Returns the assembled messages (L1 summary + recent) and the L1 summary
    /// text for potential L2 cascade.
    async fn try_l1_compaction(
        &self,
        provider: &dyn ProviderAdapter,
        storage: &dyn StorageAdapter,
        session_id: &str,
        older: &[blufio_core::types::Message],
        recent: &[blufio_core::types::Message],
        _model: &str,
        compaction_usages: &mut Vec<TokenUsage>,
        extracted_entities: &mut Vec<String>,
    ) -> Result<(Vec<ProviderMessage>, String), BlufioError> {
        // Emit CompactionStarted event.
        self.emit_compaction_started(session_id, "L1", older.len() as u32)
            .await;
        let start = std::time::Instant::now();

        // Entity extraction BEFORE L1 compaction (non-blocking on failure).
        match extract_entities(provider, older, &self.compaction_model).await {
            Ok(ExtractionOutput { entities, usage }) => {
                *extracted_entities = entities;
                compaction_usages.push(usage);
                info!(
                    entity_count = extracted_entities.len(),
                    "entity extraction completed before L1"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "entity extraction failed, continuing with compaction"
                );
            }
        }

        // L1 compaction: turn-pair bullet summaries.
        let mut l1_result =
            compact_to_l1(provider, older, &self.compaction_model, self.max_tokens_l1).await?;

        compaction_usages.push(l1_result.usage.clone());

        // Quality scoring after L1 compaction.
        if self.quality_scoring {
            let quality_outcome = self
                .apply_quality_scoring(provider, older, &l1_result.summary, compaction_usages)
                .await;

            match quality_outcome {
                QualityOutcome::Proceed(score) => {
                    l1_result.quality_score = Some(score);
                }
                QualityOutcome::RetryNeeded(weakest) => {
                    // Re-compact with emphasis on weakest dimension.
                    info!(
                        weakest = %weakest,
                        "quality gate retry: re-compacting L1 with emphasis"
                    );
                    match compact_to_l1(provider, older, &self.compaction_model, self.max_tokens_l1)
                        .await
                    {
                        Ok(retry_result) => {
                            compaction_usages.push(retry_result.usage.clone());
                            // Evaluate the retry result.
                            match evaluate_and_gate(
                                provider,
                                older,
                                &retry_result.summary,
                                &self.compaction_model,
                                &self.quality_weights,
                                self.quality_gate_proceed,
                                self.quality_gate_retry,
                            )
                            .await
                            {
                                Ok((score, GateResult::Proceed(_))) => {
                                    l1_result = retry_result;
                                    l1_result.quality_score = Some(score);
                                }
                                Ok((_, _)) => {
                                    // Still retry/abort after retry: treat as abort.
                                    warn!(
                                        "L1 quality still insufficient after retry, aborting compaction"
                                    );
                                    return Err(BlufioError::Internal(
                                        "L1 compaction quality gate abort after retry".to_string(),
                                    ));
                                }
                                Err(e) => {
                                    warn!(error = %e, "quality re-evaluation failed, continuing with original");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "L1 retry compaction failed, continuing with original");
                        }
                    }
                }
                QualityOutcome::Abort => {
                    warn!("L1 compaction quality gate abort, falling back to truncation");
                    return Err(BlufioError::Internal(
                        "L1 compaction quality gate abort".to_string(),
                    ));
                }
                QualityOutcome::ScoringFailed => {
                    // Continue without quality score.
                }
            }
        }

        // Persist L1 summary with level metadata.
        let l1_msg_id = persist_compaction_summary_with_level(
            storage,
            session_id,
            &l1_result.summary,
            older.len(),
            &CompactionLevel::L1,
            l1_result.quality_score,
        )
        .await?;

        // Delete original compacted messages.
        let older_ids: Vec<String> = older.iter().map(|m| m.id.clone()).collect();
        if let Err(e) = storage.delete_messages_by_ids(session_id, &older_ids).await {
            warn!(error = %e, "failed to delete compacted messages, continuing");
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        // Emit CompactionCompleted event.
        self.emit_compaction_completed(
            session_id,
            "L1",
            l1_result.quality_score.unwrap_or(0.0),
            l1_result.tokens_saved as u32,
            duration_ms,
        )
        .await;

        // Build messages: L1 summary + recent messages.
        let mut msgs = vec![ProviderMessage {
            role: "system".to_string(),
            content: vec![ContentBlock::Text {
                text: l1_result.summary.clone(),
            }],
        }];

        for msg in recent {
            msgs.push(ProviderMessage {
                role: msg.role.clone(),
                content: vec![ContentBlock::Text {
                    text: msg.content.clone(),
                }],
            });
        }

        // Return L1 summary text and message ID for potential L2 cascade.
        let _ = l1_msg_id; // Used for potential deletion in L2 cascade
        Ok((msgs, l1_result.summary))
    }

    /// Attempts L2 cascade: compresses L1 bullet summaries into a session narrative.
    ///
    /// Returns messages starting with the L2 summary (caller adds recent messages).
    async fn try_l2_cascade(
        &self,
        provider: &dyn ProviderAdapter,
        storage: &dyn StorageAdapter,
        session_id: &str,
        l1_summary_text: &str,
        compaction_usages: &mut Vec<TokenUsage>,
    ) -> Result<Vec<ProviderMessage>, BlufioError> {
        // Emit CompactionStarted for L2.
        self.emit_compaction_started(session_id, "L2", 1).await;
        let start = std::time::Instant::now();

        let mut l2_result = compact_to_l2(
            provider,
            l1_summary_text,
            &self.compaction_model,
            self.max_tokens_l2,
        )
        .await?;

        compaction_usages.push(l2_result.usage.clone());

        // Quality scoring after L2 compaction.
        // For L2, we don't have the original raw messages available (they were replaced
        // by L1 summary), so we evaluate the L2 summary against the L1 summary text.
        // This is less precise but still catches major quality regressions.
        if self.quality_scoring {
            // Build a synthetic message from L1 text to evaluate against.
            let l1_as_messages = vec![blufio_core::types::Message {
                id: String::new(),
                session_id: String::new(),
                role: "system".to_string(),
                content: l1_summary_text.to_string(),
                token_count: None,
                metadata: None,
                created_at: String::new(),
                classification: Default::default(),
            }];

            let quality_outcome = self
                .apply_quality_scoring(
                    provider,
                    &l1_as_messages,
                    &l2_result.summary,
                    compaction_usages,
                )
                .await;

            match quality_outcome {
                QualityOutcome::Proceed(score) => {
                    l2_result.quality_score = Some(score);
                }
                QualityOutcome::RetryNeeded(weakest) => {
                    info!(
                        weakest = %weakest,
                        "quality gate retry: re-compacting L2 with emphasis"
                    );
                    match compact_to_l2(
                        provider,
                        l1_summary_text,
                        &self.compaction_model,
                        self.max_tokens_l2,
                    )
                    .await
                    {
                        Ok(retry_result) => {
                            compaction_usages.push(retry_result.usage.clone());
                            match evaluate_and_gate(
                                provider,
                                &l1_as_messages,
                                &retry_result.summary,
                                &self.compaction_model,
                                &self.quality_weights,
                                self.quality_gate_proceed,
                                self.quality_gate_retry,
                            )
                            .await
                            {
                                Ok((score, GateResult::Proceed(_))) => {
                                    l2_result = retry_result;
                                    l2_result.quality_score = Some(score);
                                }
                                Ok((_, _)) => {
                                    warn!(
                                        "L2 quality still insufficient after retry, continuing with original"
                                    );
                                    // For L2 we don't abort: the L1 summary still exists.
                                }
                                Err(e) => {
                                    warn!(error = %e, "L2 quality re-evaluation failed, continuing");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "L2 retry compaction failed, continuing with original");
                        }
                    }
                }
                QualityOutcome::Abort => {
                    warn!("L2 compaction quality gate abort, continuing with L1 summary");
                    return Err(BlufioError::Internal(
                        "L2 compaction quality gate abort".to_string(),
                    ));
                }
                QualityOutcome::ScoringFailed => {
                    // Continue without quality score.
                }
            }
        }

        // Replace L1 summary with L2 summary in storage.
        persist_compaction_summary_with_level(
            storage,
            session_id,
            &l2_result.summary,
            0, // L2 compacts L1 summaries, not raw messages.
            &CompactionLevel::L2,
            l2_result.quality_score,
        )
        .await?;

        let duration_ms = start.elapsed().as_millis() as u64;
        self.emit_compaction_completed(
            session_id,
            "L2",
            l2_result.quality_score.unwrap_or(0.0),
            l2_result.tokens_saved as u32,
            duration_ms,
        )
        .await;

        // Build messages with L2 summary.
        let msgs = vec![ProviderMessage {
            role: "system".to_string(),
            content: vec![ContentBlock::Text {
                text: l2_result.summary,
            }],
        }];

        Ok(msgs)
    }

    /// Applies quality scoring to a compaction result and returns the outcome.
    ///
    /// This is a helper that evaluates quality and categorizes the result into
    /// one of four outcomes. The caller handles each outcome appropriately.
    async fn apply_quality_scoring(
        &self,
        provider: &dyn ProviderAdapter,
        original_messages: &[blufio_core::types::Message],
        summary: &str,
        _compaction_usages: &mut Vec<TokenUsage>,
    ) -> QualityOutcome {
        match evaluate_and_gate(
            provider,
            original_messages,
            summary,
            &self.compaction_model,
            &self.quality_weights,
            self.quality_gate_proceed,
            self.quality_gate_retry,
        )
        .await
        {
            Ok((score, GateResult::Proceed(_))) => QualityOutcome::Proceed(score),
            Ok((_, GateResult::Retry(_, weakest))) => QualityOutcome::RetryNeeded(weakest),
            Ok((_, GateResult::Abort(_))) => QualityOutcome::Abort,
            Err(e) => {
                warn!(error = %e, "quality scoring failed, continuing without score");
                QualityOutcome::ScoringFailed
            }
        }
    }

    /// Truncates history to fit within budget (fallback when compaction fails).
    async fn truncate_to_budget(
        &self,
        history: &[blufio_core::types::Message],
        target_tokens: usize,
        counter: &dyn blufio_core::token_counter::TokenCounter,
        inbound: &InboundMessage,
    ) -> Vec<ProviderMessage> {
        // Keep the most recent messages that fit within budget.
        let mut kept = Vec::new();
        let mut token_count: usize = 0;

        for msg in history.iter().rev() {
            let msg_tokens = count_with_fallback(counter, &msg.content).await;
            if token_count + msg_tokens > target_tokens && !kept.is_empty() {
                break;
            }
            token_count += msg_tokens;
            kept.push(ProviderMessage {
                role: msg.role.clone(),
                content: vec![ContentBlock::Text {
                    text: msg.content.clone(),
                }],
            });
        }

        // Reverse to restore chronological order.
        kept.reverse();

        warn!(
            original_count = history.len(),
            kept_count = kept.len(),
            "truncated history as compaction fallback"
        );

        // Append inbound message.
        let inbound_content = message_content_to_blocks(&inbound.content);
        kept.push(ProviderMessage {
            role: "user".to_string(),
            content: inbound_content,
        });

        kept
    }

    /// Emits a CompactionStarted event via the EventBus (if present).
    async fn emit_compaction_started(&self, session_id: &str, level: &str, message_count: u32) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(BusEvent::Compaction(CompactionEvent::Started {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                session_id: session_id.to_string(),
                level: level.to_lowercase(),
                message_count,
            }))
            .await;
        }
    }

    /// Emits a CompactionCompleted event via the EventBus (if present).
    async fn emit_compaction_completed(
        &self,
        session_id: &str,
        level: &str,
        quality_score: f64,
        tokens_saved: u32,
        duration_ms: u64,
    ) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(BusEvent::Compaction(CompactionEvent::Completed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                session_id: session_id.to_string(),
                level: level.to_lowercase(),
                quality_score,
                tokens_saved,
                duration_ms,
            }))
            .await;
        }
    }
}

/// Converts a [`MessageContent`] into provider [`ContentBlock`]s.
///
/// Duplicated from blufio-agent/context.rs to avoid circular dependency
/// (blufio-context should NOT depend on blufio-agent).
fn message_content_to_blocks(content: &MessageContent) -> Vec<ContentBlock> {
    match content {
        MessageContent::Text(text) => vec![ContentBlock::Text { text: text.clone() }],
        MessageContent::Image {
            data,
            mime_type,
            caption,
        } => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
            let mut blocks = vec![ContentBlock::Image {
                source_type: "base64".to_string(),
                media_type: mime_type.clone(),
                data: encoded,
            }];
            if let Some(cap) = caption {
                blocks.push(ContentBlock::Text { text: cap.clone() });
            }
            blocks
        }
        MessageContent::Document {
            data: _,
            filename,
            mime_type,
        } => {
            let desc = if mime_type.starts_with("text/") {
                format!("[Document: {filename}]")
            } else {
                format!("[Document: {filename} ({mime_type}) - binary content attached]")
            };
            vec![ContentBlock::Text { text: desc }]
        }
        MessageContent::Voice { duration_secs, .. } => {
            let duration_str = duration_secs
                .map(|d| format!("{d:.0}s"))
                .unwrap_or_else(|| "unknown duration".to_string());
            vec![ContentBlock::Text {
                text: format!("[Voice message, {duration_str} - transcription pending]"),
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_zone_new_from_config() {
        use blufio_core::token_counter::{TokenizerCache, TokenizerMode};
        let config = ContextConfig::default();
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));
        let zone = DynamicZone::new(&config, cache);
        assert_eq!(zone.soft_trigger, 0.50);
        assert_eq!(zone.hard_trigger, 0.85);
        assert_eq!(zone.context_budget, 180_000);
        assert_eq!(zone.compaction_model, "claude-haiku-4-5-20250901");
        assert!(zone.compaction_enabled);
        assert_eq!(zone.max_tokens_l1, 256);
        assert_eq!(zone.max_tokens_l2, 1024);
        assert!(zone.event_bus.is_none());
    }

    #[test]
    fn dynamic_zone_with_event_bus() {
        use blufio_core::token_counter::{TokenizerCache, TokenizerMode};
        let config = ContextConfig::default();
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));
        let bus = Arc::new(EventBus::new(16));
        let zone = DynamicZone::with_event_bus(&config, cache, bus);
        assert!(zone.event_bus.is_some());
        assert_eq!(zone.soft_trigger, 0.50);
    }

    #[test]
    fn text_content_to_blocks() {
        let content = MessageContent::Text("hello".to_string());
        let blocks = message_content_to_blocks(&content);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn voice_content_to_blocks() {
        let content = MessageContent::Voice {
            data: vec![],
            duration_secs: Some(5.0),
        };
        let blocks = message_content_to_blocks(&content);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("Voice message"));
                assert!(text.contains("5s"));
            }
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn dynamic_result_without_compaction() {
        let result = DynamicResult {
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
            }],
            compaction_usages: vec![],
            extracted_entities: vec![],
        };
        assert!(result.compaction_usages.is_empty());
        assert!(result.extracted_entities.is_empty());
        assert_eq!(result.messages.len(), 1);
    }

    #[test]
    fn dynamic_result_with_compaction() {
        let result = DynamicResult {
            messages: vec![],
            compaction_usages: vec![TokenUsage {
                input_tokens: 500,
                output_tokens: 100,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            }],
            extracted_entities: vec!["Alice is the project lead".to_string()],
        };
        assert_eq!(result.compaction_usages.len(), 1);
        assert_eq!(result.compaction_usages[0].input_tokens, 500);
        assert_eq!(result.extracted_entities.len(), 1);
    }

    #[test]
    fn dynamic_result_with_cascade_compaction() {
        let result = DynamicResult {
            messages: vec![],
            compaction_usages: vec![
                TokenUsage {
                    input_tokens: 500,
                    output_tokens: 100,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                },
                TokenUsage {
                    input_tokens: 200,
                    output_tokens: 80,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                },
            ],
            extracted_entities: vec![],
        };
        assert_eq!(result.compaction_usages.len(), 2);
        assert_eq!(result.compaction_usages[0].input_tokens, 500);
        assert_eq!(result.compaction_usages[1].input_tokens, 200);
    }

    #[test]
    fn dynamic_zone_disabled_compaction() {
        use blufio_core::token_counter::{TokenizerCache, TokenizerMode};
        let mut config = ContextConfig::default();
        config.compaction_enabled = false;
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));
        let zone = DynamicZone::new(&config, cache);
        assert!(!zone.compaction_enabled);
    }
}
