// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebSocket connection manager for the node mesh.
//!
//! Manages outbound connections to paired nodes (via tokio-tungstenite)
//! and tracks active connections in a DashMap for message routing.
//! Handles reconnection with exponential backoff and jitter.

use std::sync::Arc;
use std::time::Duration;

use blufio_bus::{
    EventBus,
    events::{BusEvent, NodeEvent, new_event_id, now_timestamp},
};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::NodeConfig;
use crate::store::NodeStore;
use crate::types::{NodeId, NodeInfo, NodeMessage, NodeStatus};

/// Manages WebSocket connections to paired nodes.
pub struct ConnectionManager {
    /// Active connections: node_id -> message sender channel.
    connections: Arc<DashMap<NodeId, mpsc::Sender<NodeMessage>>>,
    /// Runtime node info (status, metrics) - updated by heartbeat receiver.
    node_states: Arc<DashMap<NodeId, NodeRuntimeState>>,
    /// Persistent store for pairings.
    store: Arc<NodeStore>,
    /// Event bus for node lifecycle events.
    event_bus: Arc<EventBus>,
    /// Node configuration.
    config: NodeConfig,
    /// Approval router for handling incoming approval responses.
    /// Uses OnceLock for Arc-compatible set-once initialization.
    approval_router: std::sync::OnceLock<Arc<crate::approval::ApprovalRouter>>,
}

/// Runtime state for a connected node (not persisted).
#[derive(Debug, Clone)]
pub struct NodeRuntimeState {
    pub status: NodeStatus,
    pub last_heartbeat: std::time::Instant,
    pub battery_percent: Option<u8>,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub uptime_secs: u64,
}

impl ConnectionManager {
    /// Create a new connection manager.
    pub fn new(store: Arc<NodeStore>, event_bus: Arc<EventBus>, config: NodeConfig) -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            node_states: Arc::new(DashMap::new()),
            store,
            event_bus,
            config,
            approval_router: std::sync::OnceLock::new(),
        }
    }

    /// Get the active connections map (for message sending).
    pub fn connections(&self) -> &Arc<DashMap<NodeId, mpsc::Sender<NodeMessage>>> {
        &self.connections
    }

    /// Get the runtime node states map.
    pub fn node_states(&self) -> &Arc<DashMap<NodeId, NodeRuntimeState>> {
        &self.node_states
    }

    /// Set the approval router for handling incoming approval responses.
    ///
    /// Takes `&self` (not `&mut self`) so it can be called on `Arc<ConnectionManager>`.
    /// Uses `OnceLock` internally -- only the first call has effect.
    pub fn set_approval_router(&self, router: Arc<crate::approval::ApprovalRouter>) {
        let _ = self.approval_router.set(router);
    }

    /// Connect to all known paired nodes on startup.
    pub async fn reconnect_all(&self) {
        let peers = match self.store.list_pairings().await {
            Ok(p) => p,
            Err(e) => {
                error!("failed to load pairings for reconnection: {e}");
                return;
            }
        };

        for peer in peers {
            if let Some(endpoint) = &peer.endpoint {
                let connections = self.connections.clone();
                let node_states = self.node_states.clone();
                let event_bus = self.event_bus.clone();
                let store = self.store.clone();
                let config = self.config.clone();
                let endpoint = endpoint.clone();
                let peer = peer.clone();
                let approval_router = self.approval_router.get().cloned();

                tokio::spawn(async move {
                    reconnect_with_backoff(
                        &peer,
                        &endpoint,
                        connections,
                        node_states,
                        event_bus,
                        store,
                        &config,
                        approval_router,
                    )
                    .await;
                });
            }
        }
    }

    /// Send a message to a specific connected node.
    pub async fn send_to(
        &self,
        node_id: &str,
        message: NodeMessage,
    ) -> Result<(), crate::NodeError> {
        if let Some(sender) = self.connections.get(node_id) {
            sender
                .send(message)
                .await
                .map_err(|e| crate::NodeError::Connection(format!("send failed: {e}")))?;
            Ok(())
        } else {
            Err(crate::NodeError::Connection(format!(
                "node {node_id} is not connected"
            )))
        }
    }

    /// Send a message to all connected nodes.
    pub async fn broadcast(&self, message: NodeMessage) {
        for entry in self.connections.iter() {
            let node_id = entry.key().clone();
            if let Err(e) = entry.value().send(message.clone()).await {
                warn!(node_id = %node_id, "broadcast send failed: {e}");
            }
        }
    }

    /// Check if a node has a specific capability.
    pub async fn has_capability(
        &self,
        node_id: &str,
        capability: &str,
    ) -> Result<bool, crate::NodeError> {
        match self.store.get_pairing(node_id).await? {
            Some(info) => Ok(info
                .capabilities
                .iter()
                .any(|c| c.to_string() == capability)),
            None => Err(crate::NodeError::Connection(format!(
                "unknown node: {node_id}"
            ))),
        }
    }

    /// Get enriched node info with runtime state merged.
    pub async fn list_nodes_with_state(&self) -> Result<Vec<NodeInfo>, crate::NodeError> {
        let mut nodes = self.store.list_pairings().await?;
        let stale_threshold = Duration::from_secs(self.config.heartbeat.stale_threshold_secs);

        for node in &mut nodes {
            if let Some(state) = self.node_states.get(&node.node_id) {
                let elapsed = state.last_heartbeat.elapsed();
                node.status = if elapsed > stale_threshold {
                    NodeStatus::Stale
                } else {
                    NodeStatus::Online
                };
                node.battery_percent = state.battery_percent;
                node.memory_used_mb = Some(state.memory_used_mb);
                node.memory_total_mb = Some(state.memory_total_mb);
            } else if self.connections.contains_key(&node.node_id) {
                node.status = NodeStatus::Online;
            }
            // else: status remains Offline (default from store)
        }

        Ok(nodes)
    }

    /// Register an accepted incoming connection (from server-side WebSocket handler).
    pub async fn register_connection(&self, node_id: NodeId, sender: mpsc::Sender<NodeMessage>) {
        self.connections.insert(node_id.clone(), sender);
        self.node_states.insert(
            node_id.clone(),
            NodeRuntimeState {
                status: NodeStatus::Online,
                last_heartbeat: std::time::Instant::now(),
                battery_percent: None,
                memory_used_mb: 0,
                memory_total_mb: 0,
                uptime_secs: 0,
            },
        );

        self.event_bus
            .publish(BusEvent::Node(NodeEvent::Connected {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id,
            }))
            .await;
    }

    /// Remove a connection (on disconnect).
    pub async fn remove_connection(&self, node_id: &str, reason: &str) {
        self.connections.remove(node_id);
        self.node_states.remove(node_id);

        self.event_bus
            .publish(BusEvent::Node(NodeEvent::Disconnected {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id: node_id.to_string(),
                reason: reason.to_string(),
            }))
            .await;
    }

    /// Update runtime state from a heartbeat message.
    pub fn update_heartbeat(
        &self,
        node_id: &str,
        battery_percent: Option<u8>,
        memory_used_mb: u64,
        memory_total_mb: u64,
        uptime_secs: u64,
    ) {
        self.node_states.insert(
            node_id.to_string(),
            NodeRuntimeState {
                status: NodeStatus::Online,
                last_heartbeat: std::time::Instant::now(),
                battery_percent,
                memory_used_mb,
                memory_total_mb,
                uptime_secs,
            },
        );
    }
}

/// Reconnect to a peer with exponential backoff and jitter.
async fn reconnect_with_backoff(
    peer: &NodeInfo,
    endpoint: &str,
    connections: Arc<DashMap<NodeId, mpsc::Sender<NodeMessage>>>,
    node_states: Arc<DashMap<NodeId, NodeRuntimeState>>,
    event_bus: Arc<EventBus>,
    store: Arc<NodeStore>,
    config: &NodeConfig,
    approval_router: Option<Arc<crate::approval::ApprovalRouter>>,
) {
    let mut delay = Duration::from_secs(config.reconnect.initial_delay_secs);
    let max_delay = Duration::from_secs(config.reconnect.max_delay_secs);
    let mut attempts = 0u32;

    loop {
        match connect_to_peer(endpoint, &config.node_id, config).await {
            Ok((sender, mut receiver)) => {
                info!(node_id = %peer.node_id, "connected to peer");
                connections.insert(peer.node_id.clone(), sender);
                node_states.insert(
                    peer.node_id.clone(),
                    NodeRuntimeState {
                        status: NodeStatus::Online,
                        last_heartbeat: std::time::Instant::now(),
                        battery_percent: None,
                        memory_used_mb: 0,
                        memory_total_mb: 0,
                        uptime_secs: 0,
                    },
                );

                event_bus
                    .publish(BusEvent::Node(NodeEvent::Connected {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        node_id: peer.node_id.clone(),
                    }))
                    .await;

                // Update last_seen in store
                let _ = store
                    .update_last_seen(&peer.node_id, &now_timestamp())
                    .await;

                // Message receive loop
                while let Some(msg) = receiver.recv().await {
                    match msg {
                        NodeMessage::Heartbeat {
                            ref node_id,
                            battery_percent,
                            memory_used_mb,
                            memory_total_mb,
                            uptime_secs,
                        } => {
                            node_states.insert(
                                node_id.clone(),
                                NodeRuntimeState {
                                    status: NodeStatus::Online,
                                    last_heartbeat: std::time::Instant::now(),
                                    battery_percent,
                                    memory_used_mb,
                                    memory_total_mb,
                                    uptime_secs,
                                },
                            );
                            let _ = store.update_last_seen(node_id, &now_timestamp()).await;
                        }
                        NodeMessage::ApprovalResponse {
                            ref request_id,
                            approved,
                            ref responder_node,
                        } => {
                            debug!(
                                request_id = %request_id,
                                approved = approved,
                                responder = %responder_node,
                                "received approval response from peer"
                            );
                            if let Some(ref router) = approval_router {
                                match router
                                    .handle_response(request_id, approved, responder_node)
                                    .await
                                {
                                    Ok(was_first) => {
                                        debug!(
                                            request_id = %request_id,
                                            was_first = was_first,
                                            "approval response forwarded"
                                        );
                                    }
                                    Err(e) => {
                                        warn!(
                                            request_id = %request_id,
                                            error = %e,
                                            "failed to handle approval response"
                                        );
                                    }
                                }
                            }
                        }
                        NodeMessage::ApprovalHandled {
                            ref request_id,
                            ref handled_by,
                        } => {
                            info!(
                                request_id = %request_id,
                                handled_by = %handled_by,
                                "approval already handled by another device"
                            );
                        }
                        _ => {
                            debug!(node_id = %peer.node_id, msg_type = ?std::mem::discriminant(&msg), "received node message");
                        }
                    }
                }

                // Connection dropped, remove and retry
                connections.remove(&peer.node_id);
                node_states.remove(&peer.node_id);
                event_bus
                    .publish(BusEvent::Node(NodeEvent::Disconnected {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        node_id: peer.node_id.clone(),
                        reason: "connection closed".to_string(),
                    }))
                    .await;

                // Reset backoff after a successful connection
                delay = Duration::from_secs(config.reconnect.initial_delay_secs);
                attempts = 0;
            }
            Err(e) => {
                attempts += 1;
                warn!(
                    node_id = %peer.node_id,
                    attempt = attempts,
                    delay_secs = delay.as_secs(),
                    "reconnection failed: {e}"
                );
            }
        }

        // Apply backoff with optional jitter
        let mut actual_delay = delay;
        if config.reconnect.jitter {
            use rand::Rng;
            let jitter = rand::thread_rng().gen_range(0..=delay.as_millis() as u64 / 4);
            actual_delay += Duration::from_millis(jitter);
        }
        tokio::time::sleep(actual_delay).await;
        delay = (delay * 2).min(max_delay);
    }
}

/// Establish a WebSocket client connection to a peer node.
///
/// Returns a sender channel for outgoing messages and a receiver for incoming.
async fn connect_to_peer(
    endpoint: &str,
    our_node_id: &str,
    config: &NodeConfig,
) -> Result<(mpsc::Sender<NodeMessage>, mpsc::Receiver<NodeMessage>), crate::NodeError> {
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let (ws_stream, _response) = connect_async(endpoint)
        .await
        .map_err(|e| crate::NodeError::Connection(format!("WebSocket connect failed: {e}")))?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send Hello message
    let hello = NodeMessage::Hello {
        node_id: our_node_id.to_string(),
        capabilities: config.capabilities.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let hello_json = serde_json::to_string(&hello)
        .map_err(|e| crate::NodeError::Connection(format!("serialize hello: {e}")))?;
    ws_sender
        .send(Message::Text(hello_json.into()))
        .await
        .map_err(|e| crate::NodeError::Connection(format!("send hello: {e}")))?;

    // Create channels for the caller
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<NodeMessage>(64);
    let (incoming_tx, incoming_rx) = mpsc::channel::<NodeMessage>(64);

    // Spawn sender task: forwards messages from outgoing channel to WebSocket
    tokio::spawn(async move {
        while let Some(msg) = outgoing_rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if ws_sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("failed to serialize outgoing node message: {e}");
                }
            }
        }
    });

    // Spawn receiver task: reads from WebSocket and forwards to incoming channel
    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Text(text) => {
                    let text_str: &str = &text;
                    match serde_json::from_str::<NodeMessage>(text_str) {
                        Ok(node_msg) => {
                            if incoming_tx.send(node_msg).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("invalid node message: {e}");
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    Ok((outgoing_tx, incoming_rx))
}
