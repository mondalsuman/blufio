// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Approval routing for the node mesh.
//!
//! Broadcasts approval requests to all connected operator devices and
//! handles first-wins resolution with timeout-then-deny fallback.
//!
//! Design:
//! - Approval requests are stored in SQLite for durability
//! - Active approvals tracked in DashMap for atomic first-wins resolution
//! - Timeout task auto-denies after configurable period (default 5 min)
//! - Losing devices notified via ApprovalHandled message

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};

use crate::config::NodeApprovalConfig;
use crate::connection::ConnectionManager;
use crate::store::NodeStore;
use crate::types::{ApprovalStatus, NodeMessage};

/// State of an active (in-flight) approval request.
#[derive(Debug)]
struct ActiveApproval {
    /// Current status (Pending until resolved).
    status: ApprovalStatus,
    /// Signal to notify the requester of the outcome.
    result_sender: Option<oneshot::Sender<ApprovalOutcome>>,
}

/// Outcome of an approval request.
#[derive(Debug, Clone)]
pub struct ApprovalOutcome {
    /// The request ID.
    pub request_id: String,
    /// Whether the request was approved.
    pub approved: bool,
    /// Node that handled the request (None if expired).
    pub handled_by: Option<String>,
}

/// Routes approval requests to connected operator devices.
pub struct ApprovalRouter {
    /// In-flight approval state: request_id -> active approval.
    active: Arc<DashMap<String, ActiveApproval>>,
    /// Connection manager for broadcasting messages.
    conn_manager: Arc<ConnectionManager>,
    /// Persistent store for approval records.
    store: Arc<NodeStore>,
    /// Approval configuration.
    config: NodeApprovalConfig,
}

impl ApprovalRouter {
    /// Create a new approval router.
    pub fn new(
        conn_manager: Arc<ConnectionManager>,
        store: Arc<NodeStore>,
        config: NodeApprovalConfig,
    ) -> Self {
        Self {
            active: Arc::new(DashMap::new()),
            conn_manager,
            store,
            config,
        }
    }

    /// Check if an action type requires broadcast approval.
    pub fn requires_approval(&self, action_type: &str) -> bool {
        self.config
            .broadcast_actions
            .iter()
            .any(|a| a == action_type)
    }

    /// Request approval from all connected operator devices.
    ///
    /// Returns a oneshot receiver that will fire when the approval is resolved
    /// (approved, denied, or expired after timeout).
    pub async fn request_approval(
        &self,
        action_type: &str,
        description: &str,
    ) -> Result<oneshot::Receiver<ApprovalOutcome>, crate::NodeError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let timeout_secs = self.config.timeout_secs;

        // Persist the approval request
        self.store
            .save_approval(&request_id, action_type, description, timeout_secs)
            .await?;

        // Create result channel
        let (tx, rx) = oneshot::channel();

        // Track as active
        self.active.insert(
            request_id.clone(),
            ActiveApproval {
                status: ApprovalStatus::Pending,
                result_sender: Some(tx),
            },
        );

        // Broadcast to all connected devices
        let message = NodeMessage::ApprovalRequest {
            request_id: request_id.clone(),
            action_type: action_type.to_string(),
            description: description.to_string(),
            timeout_secs,
        };
        self.conn_manager.broadcast(message).await;

        info!(
            request_id = %request_id,
            action_type = %action_type,
            timeout_secs = timeout_secs,
            "approval request broadcast to all connected nodes"
        );

        // Spawn timeout task
        let active = self.active.clone();
        let store = self.store.clone();
        let conn_manager = self.conn_manager.clone();
        let rid = request_id.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(timeout_secs)).await;

            // Check if still pending (atomic check-and-remove)
            if let Some((_, mut approval)) = active.remove(&rid) {
                if approval.status == ApprovalStatus::Pending {
                    debug!(request_id = %rid, "approval timed out, auto-denying");

                    // Update store
                    let _ = store
                        .resolve_approval(&rid, ApprovalStatus::Expired, None)
                        .await;

                    // Notify requester
                    if let Some(sender) = approval.result_sender.take() {
                        let _ = sender.send(ApprovalOutcome {
                            request_id: rid.clone(),
                            approved: false,
                            handled_by: None,
                        });
                    }

                    // Notify all devices that the request expired
                    conn_manager
                        .broadcast(NodeMessage::ApprovalHandled {
                            request_id: rid,
                            handled_by: "timeout".to_string(),
                        })
                        .await;
                }
            }
        });

        Ok(rx)
    }

    /// Handle an approval response from a device.
    ///
    /// Implements first-wins semantics: the first device to respond transitions
    /// the approval from Pending to Approved/Denied. All other responses receive
    /// an ApprovalHandled notification.
    pub async fn handle_response(
        &self,
        request_id: &str,
        approved: bool,
        responder_node: &str,
    ) -> Result<bool, crate::NodeError> {
        // Atomic first-wins: try to remove from active map
        if let Some((_, mut approval)) = self.active.remove(request_id) {
            if approval.status != ApprovalStatus::Pending {
                // Already resolved (shouldn't happen if removed atomically, but be safe)
                warn!(
                    request_id = %request_id,
                    "approval already resolved, ignoring duplicate response"
                );
                return Ok(false);
            }

            let status = if approved {
                ApprovalStatus::Approved
            } else {
                ApprovalStatus::Denied
            };

            // Update persistent store
            self.store
                .resolve_approval(request_id, status, Some(responder_node))
                .await?;

            info!(
                request_id = %request_id,
                approved = approved,
                handled_by = %responder_node,
                "approval resolved (first-wins)"
            );

            // Notify the requester
            if let Some(sender) = approval.result_sender.take() {
                let _ = sender.send(ApprovalOutcome {
                    request_id: request_id.to_string(),
                    approved,
                    handled_by: Some(responder_node.to_string()),
                });
            }

            // Notify all other devices that this request was handled
            self.conn_manager
                .broadcast(NodeMessage::ApprovalHandled {
                    request_id: request_id.to_string(),
                    handled_by: responder_node.to_string(),
                })
                .await;

            Ok(true)
        } else {
            // Request not found in active map — already handled or expired
            debug!(
                request_id = %request_id,
                responder = %responder_node,
                "approval already handled by another device"
            );

            // Notify the late responder
            self.conn_manager
                .send_to(
                    responder_node,
                    NodeMessage::ApprovalHandled {
                        request_id: request_id.to_string(),
                        handled_by: "another device".to_string(),
                    },
                )
                .await
                .ok(); // Ignore send error — best effort

            Ok(false)
        }
    }

    /// Get the number of currently active (pending) approvals.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Check if a specific approval is still pending.
    pub fn is_pending(&self, request_id: &str) -> bool {
        self.active.contains_key(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_outcome_clone() {
        let outcome = ApprovalOutcome {
            request_id: "test-123".to_string(),
            approved: true,
            handled_by: Some("node-1".to_string()),
        };
        let cloned = outcome.clone();
        assert_eq!(cloned.request_id, "test-123");
        assert!(cloned.approved);
        assert_eq!(cloned.handled_by, Some("node-1".to_string()));
    }
}
