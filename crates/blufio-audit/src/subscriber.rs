// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Event bus subscriber that converts [`BusEvent`] into audit [`PendingEntry`] values.
//!
//! [`AuditSubscriber`] receives events from the [`EventBus`], filters them via
//! the TOML allowlist ([`EventFilter`]), converts matching events to
//! [`PendingEntry`], and forwards them to the [`AuditWriter`] for persistence.
//!
//! # Example
//!
//! ```text
//! EventBus -> AuditSubscriber (filter + convert) -> AuditWriter (batch + persist)
//! ```

use std::sync::Arc;

use blufio_bus::events::{
    ApiEvent, AuditMetaEvent, BatchEvent, BusEvent, ChannelEvent, ClassificationEvent,
    CompactionEvent, ConfigEvent, CronEvent, MemoryEvent, NodeEvent, ProviderEvent,
    ResilienceEvent, SecurityEvent, SessionEvent, SkillEvent, WebhookEvent,
};
use metrics::counter;
use tokio::sync::mpsc;
use tracing::warn;

use crate::filter::EventFilter;
use crate::models::PendingEntry;
use crate::writer::AuditWriter;

/// Subscribes to the [`EventBus`] and routes filtered events to the [`AuditWriter`].
pub struct AuditSubscriber {
    writer: Arc<AuditWriter>,
    filter: EventFilter,
}

impl AuditSubscriber {
    /// Create a new subscriber with the given writer and filter.
    pub fn new(writer: Arc<AuditWriter>, filter: EventFilter) -> Self {
        Self { writer, filter }
    }

    /// Run the subscriber loop, consuming events from the given receiver.
    ///
    /// This method runs until the receiver is closed (sender dropped or
    /// EventBus shut down). It filters events via the TOML allowlist,
    /// converts matching events to [`PendingEntry`], and sends them to
    /// the [`AuditWriter`].
    pub async fn run(self, mut rx: mpsc::Receiver<BusEvent>) {
        while let Some(event) = rx.recv().await {
            let event_type = event.event_type_string();

            if !self.filter.matches(event_type) {
                continue;
            }

            let entry = convert_to_pending_entry(&event);

            if let Err(e) = self.writer.try_send(entry) {
                counter!("blufio_audit_dropped_total").increment(1);
                warn!(
                    error = %e,
                    event_type = event_type,
                    "audit subscriber: failed to send entry to writer"
                );
            }
        }
    }
}

/// Convert a [`BusEvent`] into a [`PendingEntry`] for audit persistence.
///
/// Maps each variant to appropriate audit fields:
/// - `event_type`: dot-separated string from `event_type_string()`
/// - `action`: derived from the variant (e.g., "create", "delete", "call")
/// - `resource_type`: the domain (e.g., "session", "memory", "config")
/// - `resource_id`: the relevant identifier from the event
/// - `actor`: "system" for most events
/// - `session_id`: extracted where available
/// - `details_json`: serialized metadata
fn convert_to_pending_entry(event: &BusEvent) -> PendingEntry {
    let event_type = event.event_type_string().to_string();

    match event {
        // --- Session events ---
        BusEvent::Session(SessionEvent::Created {
            timestamp,
            session_id,
            channel,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "create".to_string(),
            resource_type: "session".to_string(),
            resource_id: session_id.clone(),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: serde_json::json!({ "channel": channel }).to_string(),
        },
        BusEvent::Session(SessionEvent::Closed {
            timestamp,
            session_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "close".to_string(),
            resource_type: "session".to_string(),
            resource_id: session_id.clone(),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: "{}".to_string(),
        },

        // --- Channel events ---
        BusEvent::Channel(ChannelEvent::MessageReceived {
            timestamp,
            channel,
            sender_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "receive".to_string(),
            resource_type: "channel".to_string(),
            resource_id: channel.clone(),
            actor: format!("user:{sender_id}"),
            session_id: String::new(),
            details_json: serde_json::json!({ "channel": channel }).to_string(),
        },
        BusEvent::Channel(ChannelEvent::MessageSent {
            timestamp, channel, ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "send".to_string(),
            resource_type: "channel".to_string(),
            resource_id: channel.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "channel": channel }).to_string(),
        },

        // --- Skill events ---
        BusEvent::Skill(SkillEvent::Invoked {
            timestamp,
            skill_name,
            session_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "invoke".to_string(),
            resource_type: "skill".to_string(),
            resource_id: skill_name.clone(),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: serde_json::json!({ "skill_name": skill_name }).to_string(),
        },
        BusEvent::Skill(SkillEvent::Completed {
            timestamp,
            skill_name,
            is_error,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "complete".to_string(),
            resource_type: "skill".to_string(),
            resource_id: skill_name.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "skill_name": skill_name, "is_error": is_error })
                .to_string(),
        },

        // --- Node events ---
        BusEvent::Node(NodeEvent::Connected {
            timestamp, node_id, ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "connect".to_string(),
            resource_type: "node".to_string(),
            resource_id: node_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: "{}".to_string(),
        },
        BusEvent::Node(NodeEvent::Disconnected {
            timestamp,
            node_id,
            reason,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "disconnect".to_string(),
            resource_type: "node".to_string(),
            resource_id: node_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "reason": reason }).to_string(),
        },
        BusEvent::Node(NodeEvent::Paired {
            timestamp,
            node_id,
            name,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "pair".to_string(),
            resource_type: "node".to_string(),
            resource_id: node_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "name": name }).to_string(),
        },
        BusEvent::Node(NodeEvent::PairingFailed {
            timestamp, reason, ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "pair_failed".to_string(),
            resource_type: "node".to_string(),
            resource_id: String::new(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "reason": reason }).to_string(),
        },
        BusEvent::Node(NodeEvent::Stale {
            timestamp,
            node_id,
            last_seen_secs_ago,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "stale".to_string(),
            resource_type: "node".to_string(),
            resource_id: node_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "last_seen_secs_ago": last_seen_secs_ago })
                .to_string(),
        },

        // --- Webhook events ---
        BusEvent::Webhook(WebhookEvent::Triggered {
            timestamp,
            webhook_id,
            event_type: wh_event_type,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "trigger".to_string(),
            resource_type: "webhook".to_string(),
            resource_id: webhook_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "event_type": wh_event_type }).to_string(),
        },
        BusEvent::Webhook(WebhookEvent::DeliveryAttempted {
            timestamp,
            webhook_id,
            status_code,
            success,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "deliver".to_string(),
            resource_type: "webhook".to_string(),
            resource_id: webhook_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "status_code": status_code,
                "success": success,
            })
            .to_string(),
        },

        // --- Batch events ---
        BusEvent::Batch(BatchEvent::Submitted {
            timestamp,
            batch_id,
            item_count,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "submit".to_string(),
            resource_type: "batch".to_string(),
            resource_id: batch_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "item_count": item_count }).to_string(),
        },
        BusEvent::Batch(BatchEvent::Completed {
            timestamp,
            batch_id,
            success_count,
            error_count,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "complete".to_string(),
            resource_type: "batch".to_string(),
            resource_id: batch_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "success_count": success_count,
                "error_count": error_count,
            })
            .to_string(),
        },

        // --- Resilience events ---
        BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
            timestamp,
            dependency,
            from_state,
            to_state,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "state_change".to_string(),
            resource_type: "circuit_breaker".to_string(),
            resource_id: dependency.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "from_state": from_state,
                "to_state": to_state,
            })
            .to_string(),
        },
        BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged {
            timestamp,
            from_level,
            to_level,
            from_name,
            to_name,
            reason,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "level_change".to_string(),
            resource_type: "degradation".to_string(),
            resource_id: String::new(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "from_level": from_level,
                "to_level": to_level,
                "from_name": from_name,
                "to_name": to_name,
                "reason": reason,
            })
            .to_string(),
        },

        // --- Classification events ---
        BusEvent::Classification(ClassificationEvent::Changed {
            timestamp,
            entity_type,
            entity_id,
            old_level,
            new_level,
            changed_by,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "change".to_string(),
            resource_type: entity_type.clone(),
            resource_id: entity_id.clone(),
            actor: changed_by.clone(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "old_level": old_level,
                "new_level": new_level,
            })
            .to_string(),
        },
        BusEvent::Classification(ClassificationEvent::PiiDetected {
            timestamp,
            entity_type,
            entity_id,
            pii_types,
            count,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "detect".to_string(),
            resource_type: entity_type.clone(),
            resource_id: entity_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "pii_types": pii_types,
                "count": count,
            })
            .to_string(),
        },
        BusEvent::Classification(ClassificationEvent::Enforced {
            timestamp,
            entity_type,
            entity_id,
            level,
            action_blocked,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "enforce".to_string(),
            resource_type: entity_type.clone(),
            resource_id: entity_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "level": level,
                "action_blocked": action_blocked,
            })
            .to_string(),
        },
        BusEvent::Classification(ClassificationEvent::BulkChanged {
            timestamp,
            entity_type,
            count,
            old_level,
            new_level,
            changed_by,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "bulk_change".to_string(),
            resource_type: entity_type.clone(),
            resource_id: String::new(),
            actor: changed_by.clone(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "count": count,
                "old_level": old_level,
                "new_level": new_level,
            })
            .to_string(),
        },

        // --- Config events ---
        BusEvent::Config(ConfigEvent::Changed {
            timestamp,
            key,
            old_value,
            new_value,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "change".to_string(),
            resource_type: "config".to_string(),
            resource_id: key.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "old_value": old_value,
                "new_value": new_value,
            })
            .to_string(),
        },
        BusEvent::Config(ConfigEvent::Reloaded {
            timestamp, source, ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "reload".to_string(),
            resource_type: "config".to_string(),
            resource_id: String::new(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "source": source }).to_string(),
        },

        // --- Memory events ---
        BusEvent::Memory(MemoryEvent::Created {
            timestamp,
            memory_id,
            source,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "create".to_string(),
            resource_type: "memory".to_string(),
            resource_id: memory_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "source": source }).to_string(),
        },
        BusEvent::Memory(MemoryEvent::Updated {
            timestamp,
            memory_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "update".to_string(),
            resource_type: "memory".to_string(),
            resource_id: memory_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: "{}".to_string(),
        },
        BusEvent::Memory(MemoryEvent::Deleted {
            timestamp,
            memory_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "delete".to_string(),
            resource_type: "memory".to_string(),
            resource_id: memory_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: "{}".to_string(),
        },
        BusEvent::Memory(MemoryEvent::Retrieved {
            timestamp,
            memory_id,
            query,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "retrieve".to_string(),
            resource_type: "memory".to_string(),
            resource_id: memory_id.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "query": query }).to_string(),
        },
        BusEvent::Memory(MemoryEvent::Evicted {
            timestamp,
            count,
            lowest_score,
            highest_score,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "evict".to_string(),
            resource_type: "memory".to_string(),
            resource_id: format!("batch:{count}"),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "count": count,
                "lowest_score": lowest_score,
                "highest_score": highest_score,
            })
            .to_string(),
        },

        // --- Audit meta-events ---
        BusEvent::Audit(AuditMetaEvent::Enabled { timestamp, .. }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "enable".to_string(),
            resource_type: "audit".to_string(),
            resource_id: String::new(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: "{}".to_string(),
        },
        BusEvent::Audit(AuditMetaEvent::Disabled { timestamp, .. }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "disable".to_string(),
            resource_type: "audit".to_string(),
            resource_id: String::new(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: "{}".to_string(),
        },
        BusEvent::Audit(AuditMetaEvent::Erased {
            timestamp,
            user_id_hash,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "erase".to_string(),
            resource_type: "audit".to_string(),
            resource_id: String::new(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({ "user_id_hash": user_id_hash }).to_string(),
        },

        // --- API events ---
        BusEvent::Api(ApiEvent::Request {
            timestamp,
            method,
            path,
            status,
            actor,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: method.to_lowercase(),
            resource_type: "api".to_string(),
            resource_id: path.clone(),
            actor: actor.clone(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "method": method,
                "path": path,
                "status": status,
            })
            .to_string(),
        },

        // --- Provider events ---
        BusEvent::Provider(ProviderEvent::Called {
            timestamp,
            provider,
            model,
            input_tokens,
            output_tokens,
            cost_usd,
            latency_ms,
            success,
            session_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "call".to_string(),
            resource_type: "provider".to_string(),
            resource_id: format!("{provider}/{model}"),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: serde_json::json!({
                "provider": provider,
                "model": model,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "cost_usd": cost_usd,
                "latency_ms": latency_ms,
                "success": success,
            })
            .to_string(),
        },

        // --- Compaction events ---
        BusEvent::Compaction(CompactionEvent::Started {
            timestamp,
            session_id,
            level,
            message_count,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "start".to_string(),
            resource_type: "compaction".to_string(),
            resource_id: session_id.clone(),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: serde_json::json!({
                "level": level,
                "message_count": message_count,
            })
            .to_string(),
        },
        BusEvent::Compaction(CompactionEvent::Completed {
            timestamp,
            session_id,
            level,
            quality_score,
            tokens_saved,
            duration_ms,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "complete".to_string(),
            resource_type: "compaction".to_string(),
            resource_id: session_id.clone(),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: serde_json::json!({
                "level": level,
                "quality_score": quality_score,
                "tokens_saved": tokens_saved,
                "duration_ms": duration_ms,
            })
            .to_string(),
        },
        BusEvent::Security(SecurityEvent::InputDetection {
            timestamp,
            correlation_id,
            source_type,
            source_name,
            score,
            action,
            categories,
            content,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: action.clone(),
            resource_type: "security".to_string(),
            resource_id: correlation_id.clone(),
            actor: format!("{}:{}", source_type, source_name),
            session_id: correlation_id.clone(),
            details_json: serde_json::json!({
                "source_type": source_type,
                "source_name": source_name,
                "score": score,
                "categories": categories,
                "content": content,
            })
            .to_string(),
        },
        BusEvent::Security(SecurityEvent::BoundaryFailure {
            timestamp,
            correlation_id,
            zone,
            source,
            action,
            content,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: action.clone(),
            resource_type: "security".to_string(),
            resource_id: correlation_id.clone(),
            actor: "system".to_string(),
            session_id: correlation_id.clone(),
            details_json: serde_json::json!({
                "zone": zone,
                "source": source,
                "content": content,
            })
            .to_string(),
        },
        BusEvent::Security(SecurityEvent::OutputScreening {
            timestamp,
            correlation_id,
            detection_type,
            tool_name,
            action,
            content,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: action.clone(),
            resource_type: "security".to_string(),
            resource_id: correlation_id.clone(),
            actor: "system".to_string(),
            session_id: correlation_id.clone(),
            details_json: serde_json::json!({
                "detection_type": detection_type,
                "tool_name": tool_name,
                "content": content,
            })
            .to_string(),
        },
        BusEvent::Security(SecurityEvent::HitlPrompt {
            timestamp,
            correlation_id,
            tool_name,
            risk_level,
            action,
            session_id,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: action.clone(),
            resource_type: "security".to_string(),
            resource_id: correlation_id.clone(),
            actor: "system".to_string(),
            session_id: session_id.clone(),
            details_json: serde_json::json!({
                "tool_name": tool_name,
                "risk_level": risk_level,
            })
            .to_string(),
        },

        // --- Cron events ---
        BusEvent::Cron(CronEvent::Completed {
            timestamp,
            job_name,
            status,
            duration_ms,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "complete".to_string(),
            resource_type: "cron".to_string(),
            resource_id: job_name.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "status": status,
                "duration_ms": duration_ms,
            })
            .to_string(),
        },
        BusEvent::Cron(CronEvent::Failed {
            timestamp,
            job_name,
            error,
            ..
        }) => PendingEntry {
            timestamp: timestamp.clone(),
            event_type,
            action: "fail".to_string(),
            resource_type: "cron".to_string(),
            resource_id: job_name.clone(),
            actor: "system".to_string(),
            session_id: String::new(),
            details_json: serde_json::json!({
                "error": error,
            })
            .to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_bus::events::{new_event_id, now_timestamp};

    #[test]
    fn convert_session_created() {
        let event = BusEvent::Session(SessionEvent::Created {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            session_id: "sess-1".into(),
            channel: "telegram".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "session.created");
        assert_eq!(entry.action, "create");
        assert_eq!(entry.resource_type, "session");
        assert_eq!(entry.resource_id, "sess-1");
        assert_eq!(entry.session_id, "sess-1");
        assert!(entry.details_json.contains("telegram"));
    }

    #[test]
    fn convert_memory_created() {
        let event = BusEvent::Memory(MemoryEvent::Created {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            memory_id: "mem-42".into(),
            source: "conversation".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "memory.created");
        assert_eq!(entry.action, "create");
        assert_eq!(entry.resource_type, "memory");
        assert_eq!(entry.resource_id, "mem-42");
        assert!(entry.details_json.contains("conversation"));
    }

    #[test]
    fn convert_provider_called() {
        let event = BusEvent::Provider(ProviderEvent::Called {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.001,
            latency_ms: 500,
            success: true,
            session_id: "sess-1".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "provider.called");
        assert_eq!(entry.action, "call");
        assert_eq!(entry.resource_type, "provider");
        assert_eq!(entry.resource_id, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(entry.session_id, "sess-1");
        assert!(entry.details_json.contains("\"input_tokens\":100"));
    }

    #[test]
    fn convert_api_request() {
        let event = BusEvent::Api(ApiEvent::Request {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            method: "POST".into(),
            path: "/v1/messages".into(),
            status: 200,
            actor: "api-key:key-1".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "api.request");
        assert_eq!(entry.action, "post");
        assert_eq!(entry.resource_type, "api");
        assert_eq!(entry.resource_id, "/v1/messages");
        assert_eq!(entry.actor, "api-key:key-1");
    }

    #[test]
    fn convert_audit_erased() {
        let event = BusEvent::Audit(AuditMetaEvent::Erased {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            user_id_hash: "abc123def456".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "audit.erased");
        assert_eq!(entry.action, "erase");
        assert_eq!(entry.resource_type, "audit");
        assert!(entry.details_json.contains("abc123def456"));
    }

    #[test]
    fn convert_config_changed() {
        let event = BusEvent::Config(ConfigEvent::Changed {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            key: "provider.model".into(),
            old_value: Some("sonnet".into()),
            new_value: Some("opus".into()),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "config.changed");
        assert_eq!(entry.action, "change");
        assert_eq!(entry.resource_type, "config");
        assert_eq!(entry.resource_id, "provider.model");
    }

    #[test]
    fn convert_classification_changed() {
        let event = BusEvent::Classification(ClassificationEvent::Changed {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            entity_type: "memory".into(),
            entity_id: "mem-1".into(),
            old_level: "internal".into(),
            new_level: "confidential".into(),
            changed_by: "auto_pii".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "classification.changed");
        assert_eq!(entry.action, "change");
        assert_eq!(entry.resource_type, "memory");
        assert_eq!(entry.resource_id, "mem-1");
        assert_eq!(entry.actor, "auto_pii");
    }

    #[test]
    fn convert_resilience_circuit_breaker() {
        let event = BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
            event_id: new_event_id(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            dependency: "anthropic".into(),
            from_state: "closed".into(),
            to_state: "open".into(),
        });
        let entry = convert_to_pending_entry(&event);
        assert_eq!(entry.event_type, "resilience.circuit_breaker_state_changed");
        assert_eq!(entry.action, "state_change");
        assert_eq!(entry.resource_type, "circuit_breaker");
        assert_eq!(entry.resource_id, "anthropic");
    }

    #[test]
    fn all_bus_event_variants_convert_successfully() {
        // Ensure convert_to_pending_entry does not panic on any variant.
        let events: Vec<BusEvent> = vec![
            BusEvent::Session(SessionEvent::Created {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                session_id: "s".into(),
                channel: "c".into(),
            }),
            BusEvent::Session(SessionEvent::Closed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                session_id: "s".into(),
            }),
            BusEvent::Channel(ChannelEvent::MessageReceived {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                channel: "c".into(),
                sender_id: "u".into(),
                content: None,
                sender_name: None,
                is_bridged: false,
            }),
            BusEvent::Channel(ChannelEvent::MessageSent {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                channel: "c".into(),
            }),
            BusEvent::Skill(SkillEvent::Invoked {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                skill_name: "s".into(),
                session_id: "sid".into(),
            }),
            BusEvent::Skill(SkillEvent::Completed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                skill_name: "s".into(),
                is_error: false,
            }),
            BusEvent::Node(NodeEvent::Connected {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id: "n".into(),
            }),
            BusEvent::Node(NodeEvent::Disconnected {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id: "n".into(),
                reason: "r".into(),
            }),
            BusEvent::Node(NodeEvent::Paired {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id: "n".into(),
                name: "nn".into(),
            }),
            BusEvent::Node(NodeEvent::PairingFailed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                reason: "r".into(),
            }),
            BusEvent::Node(NodeEvent::Stale {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id: "n".into(),
                last_seen_secs_ago: 60,
            }),
            BusEvent::Webhook(WebhookEvent::Triggered {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                webhook_id: "w".into(),
                event_type: "e".into(),
            }),
            BusEvent::Webhook(WebhookEvent::DeliveryAttempted {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                webhook_id: "w".into(),
                status_code: 200,
                success: true,
            }),
            BusEvent::Batch(BatchEvent::Submitted {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                batch_id: "b".into(),
                item_count: 1,
            }),
            BusEvent::Batch(BatchEvent::Completed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                batch_id: "b".into(),
                success_count: 1,
                error_count: 0,
            }),
            BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "d".into(),
                from_state: "closed".into(),
                to_state: "open".into(),
            }),
            BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                from_level: 0,
                to_level: 1,
                from_name: "f".into(),
                to_name: "t".into(),
                reason: "r".into(),
            }),
            BusEvent::Classification(ClassificationEvent::Changed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                entity_type: "e".into(),
                entity_id: "i".into(),
                old_level: "o".into(),
                new_level: "n".into(),
                changed_by: "c".into(),
            }),
            BusEvent::Classification(ClassificationEvent::PiiDetected {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                entity_type: "e".into(),
                entity_id: "i".into(),
                pii_types: vec![],
                count: 0,
            }),
            BusEvent::Classification(ClassificationEvent::Enforced {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                entity_type: "e".into(),
                entity_id: "i".into(),
                level: "l".into(),
                action_blocked: "a".into(),
            }),
            BusEvent::Classification(ClassificationEvent::BulkChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                entity_type: "e".into(),
                count: 0,
                old_level: "o".into(),
                new_level: "n".into(),
                changed_by: "c".into(),
            }),
            BusEvent::Config(ConfigEvent::Changed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                key: "k".into(),
                old_value: None,
                new_value: None,
            }),
            BusEvent::Config(ConfigEvent::Reloaded {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                source: "s".into(),
            }),
            BusEvent::Memory(MemoryEvent::Created {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: "m".into(),
                source: "s".into(),
            }),
            BusEvent::Memory(MemoryEvent::Updated {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: "m".into(),
            }),
            BusEvent::Memory(MemoryEvent::Deleted {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: "m".into(),
            }),
            BusEvent::Memory(MemoryEvent::Retrieved {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: "m".into(),
                query: "q".into(),
            }),
            BusEvent::Memory(MemoryEvent::Evicted {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                count: 5,
                lowest_score: 0.1,
                highest_score: 0.5,
            }),
            BusEvent::Audit(AuditMetaEvent::Enabled {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
            }),
            BusEvent::Audit(AuditMetaEvent::Disabled {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
            }),
            BusEvent::Audit(AuditMetaEvent::Erased {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                user_id_hash: "h".into(),
            }),
            BusEvent::Api(ApiEvent::Request {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                method: "POST".into(),
                path: "/v1/messages".into(),
                status: 200,
                actor: "a".into(),
            }),
            BusEvent::Provider(ProviderEvent::Called {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                provider: "p".into(),
                model: "m".into(),
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.0,
                latency_ms: 0,
                success: true,
                session_id: "s".into(),
            }),
            BusEvent::Compaction(CompactionEvent::Started {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                session_id: "s".into(),
                level: "l1".into(),
                message_count: 10,
            }),
            BusEvent::Compaction(CompactionEvent::Completed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                session_id: "s".into(),
                level: "l1".into(),
                quality_score: 0.8,
                tokens_saved: 100,
                duration_ms: 50,
            }),
            BusEvent::Security(SecurityEvent::InputDetection {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                correlation_id: "c".into(),
                source_type: "user".into(),
                source_name: "".into(),
                score: 0.5,
                action: "logged".into(),
                categories: vec!["role_hijacking".into()],
                content: "test".into(),
            }),
            BusEvent::Security(SecurityEvent::BoundaryFailure {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                correlation_id: "c".into(),
                zone: "dynamic".into(),
                source: "user".into(),
                action: "stripped".into(),
                content: "tampered".into(),
            }),
            BusEvent::Security(SecurityEvent::OutputScreening {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                correlation_id: "c".into(),
                detection_type: "credential_leak".into(),
                tool_name: "tool".into(),
                action: "redacted".into(),
                content: "sk-...".into(),
            }),
            BusEvent::Security(SecurityEvent::HitlPrompt {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                correlation_id: "c".into(),
                tool_name: "tool".into(),
                risk_level: "high".into(),
                action: "denied".into(),
                session_id: "s".into(),
            }),
            BusEvent::Cron(CronEvent::Completed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                job_name: "backup".into(),
                status: "success".into(),
                duration_ms: 100,
            }),
            BusEvent::Cron(CronEvent::Failed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                job_name: "retention".into(),
                error: "timeout".into(),
            }),
        ];

        for event in &events {
            let entry = convert_to_pending_entry(event);
            assert!(!entry.event_type.is_empty());
            assert!(!entry.action.is_empty());
            assert!(!entry.resource_type.is_empty());
        }
    }
}
