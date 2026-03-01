// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Signed inter-agent message types for multi-agent delegation.
//!
//! Provides `AgentMessage` for structured inter-agent communication and
//! `SignedAgentMessage` for Ed25519-signed message integrity verification (SEC-07).

use chrono::Utc;
use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::keypair::DeviceKeypair;
use blufio_core::BlufioError;

/// Type of inter-agent message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageType {
    /// A request from one agent to another.
    Request,
    /// A response from a specialist agent back to the requester.
    Response,
}

/// An inter-agent message payload.
///
/// All fields are included in the signed payload for integrity.
/// Serialized to canonical JSON for deterministic signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message identifier.
    pub id: String,
    /// Name of the sending agent.
    pub sender: String,
    /// Name of the receiving agent.
    pub recipient: String,
    /// Whether this is a request or response.
    pub message_type: AgentMessageType,
    /// The task description or delegation prompt.
    pub task: String,
    /// The message content or context payload.
    pub content: String,
    /// RFC 3339 timestamp of message creation.
    pub timestamp: String,
}

impl AgentMessage {
    /// Create a new request message from one agent to another.
    pub fn new_request(sender: &str, recipient: &str, task: &str, context: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            sender: sender.to_string(),
            recipient: recipient.to_string(),
            message_type: AgentMessageType::Request,
            task: task.to_string(),
            content: context.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Create a response to an existing request message.
    pub fn new_response(request: &AgentMessage, sender: &str, content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            sender: sender.to_string(),
            recipient: request.sender.clone(),
            message_type: AgentMessageType::Response,
            task: request.task.clone(),
            content: content.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Serialize to canonical JSON bytes for signing.
    ///
    /// The canonical form ensures deterministic byte representation
    /// for signature verification.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("AgentMessage is always JSON-serializable")
    }
}

/// A signed inter-agent message with Ed25519 signature.
///
/// Wraps an `AgentMessage` with a cryptographic signature over its
/// canonical JSON representation. Used for verifying message integrity
/// and sender authenticity in multi-agent delegation (SEC-07).
#[derive(Debug, Clone)]
pub struct SignedAgentMessage {
    /// The original message payload.
    pub message: AgentMessage,
    /// Ed25519 signature over the canonical bytes.
    pub signature: Signature,
    /// The exact bytes that were signed (canonical serialization).
    pub signed_bytes: Vec<u8>,
}

impl SignedAgentMessage {
    /// Create a new signed message using the sender's keypair.
    pub fn new(message: AgentMessage, keypair: &DeviceKeypair) -> Self {
        let signed_bytes = message.canonical_bytes();
        let signature = keypair.sign(&signed_bytes);
        Self {
            message,
            signature,
            signed_bytes,
        }
    }

    /// Verify this message was signed by the claimed sender.
    ///
    /// Uses strict verification (rejects weak keys) per ed25519-dalek
    /// security recommendations.
    pub fn verify(&self, sender_keypair: &DeviceKeypair) -> Result<(), BlufioError> {
        sender_keypair.verify_strict(&self.signed_bytes, &self.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_message_serializes_to_deterministic_bytes() {
        let msg = AgentMessage {
            id: "test-id-123".to_string(),
            sender: "primary".to_string(),
            recipient: "specialist".to_string(),
            message_type: AgentMessageType::Request,
            task: "summarize".to_string(),
            content: "some context".to_string(),
            timestamp: "2026-01-01T00:00:00+00:00".to_string(),
        };
        let bytes1 = msg.canonical_bytes();
        let bytes2 = msg.canonical_bytes();
        assert_eq!(bytes1, bytes2);
        assert!(!bytes1.is_empty());
    }

    #[test]
    fn signed_agent_message_signs_canonical_bytes() {
        let kp = DeviceKeypair::generate();
        let msg = AgentMessage::new_request("primary", "specialist", "task1", "context1");
        let signed = SignedAgentMessage::new(msg, &kp);

        // signed_bytes should match the message's canonical form
        assert_eq!(signed.signed_bytes, signed.message.canonical_bytes());
        // Signature should be 64 bytes
        assert_eq!(signed.signature.to_bytes().len(), 64);
    }

    #[test]
    fn signed_agent_message_verify_succeeds_with_correct_keypair() {
        let kp = DeviceKeypair::generate();
        let msg = AgentMessage::new_request("primary", "specialist", "task1", "context1");
        let signed = SignedAgentMessage::new(msg, &kp);

        assert!(signed.verify(&kp).is_ok());
    }

    #[test]
    fn signed_agent_message_verify_fails_for_tampered_content() {
        let kp = DeviceKeypair::generate();
        let msg = AgentMessage::new_request("primary", "specialist", "task1", "context1");
        let mut signed = SignedAgentMessage::new(msg, &kp);

        // Tamper with the message content after signing
        signed.message.content = "tampered content".to_string();
        // Re-serialize to get tampered bytes
        signed.signed_bytes = signed.message.canonical_bytes();

        let result = signed.verify(&kp);
        assert!(result.is_err());
        match result.unwrap_err() {
            BlufioError::Security(msg) => {
                assert!(msg.contains("Ed25519 signature verification failed"));
            }
            other => panic!("expected Security error, got: {other:?}"),
        }
    }

    #[test]
    fn signed_agent_message_verify_fails_for_wrong_keypair() {
        let kp1 = DeviceKeypair::generate();
        let kp2 = DeviceKeypair::generate();
        let msg = AgentMessage::new_request("primary", "specialist", "task1", "context1");
        let signed = SignedAgentMessage::new(msg, &kp1);

        // Verify with wrong agent's keypair should fail
        let result = signed.verify(&kp2);
        assert!(result.is_err());
    }

    #[test]
    fn agent_message_type_has_request_and_response_variants() {
        let req = AgentMessageType::Request;
        let resp = AgentMessageType::Response;
        assert_eq!(req, AgentMessageType::Request);
        assert_eq!(resp, AgentMessageType::Response);
        assert_ne!(req, resp);
    }

    #[test]
    fn agent_message_roundtrips_through_serde_json() {
        let msg = AgentMessage::new_request("sender1", "recipient1", "task1", "content1");
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.sender, deserialized.sender);
        assert_eq!(msg.recipient, deserialized.recipient);
        assert_eq!(msg.message_type, deserialized.message_type);
        assert_eq!(msg.task, deserialized.task);
        assert_eq!(msg.content, deserialized.content);
        assert_eq!(msg.timestamp, deserialized.timestamp);
    }

    #[test]
    fn new_request_creates_correct_message_type() {
        let msg = AgentMessage::new_request("primary", "specialist", "summarize", "data");
        assert_eq!(msg.message_type, AgentMessageType::Request);
        assert_eq!(msg.sender, "primary");
        assert_eq!(msg.recipient, "specialist");
        assert_eq!(msg.task, "summarize");
        assert_eq!(msg.content, "data");
        assert!(!msg.id.is_empty());
        assert!(!msg.timestamp.is_empty());
    }

    #[test]
    fn new_response_links_back_to_request() {
        let request = AgentMessage::new_request("primary", "specialist", "summarize", "data");
        let response = AgentMessage::new_response(&request, "specialist", "result");

        assert_eq!(response.message_type, AgentMessageType::Response);
        assert_eq!(response.sender, "specialist");
        assert_eq!(response.recipient, "primary"); // Links back to requester
        assert_eq!(response.task, "summarize"); // Same task
        assert_eq!(response.content, "result");
        assert_ne!(response.id, request.id); // Different ID
    }
}
