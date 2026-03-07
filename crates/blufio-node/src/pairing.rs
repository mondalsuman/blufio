// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pairing state machine for Ed25519 mutual authentication.
//!
//! Drives the pairing flow from token generation through key exchange to
//! mutual confirmation with MITM-preventing fingerprint verification.

use std::sync::Arc;

use blufio_auth_keypair::DeviceKeypair;
use blufio_bus::{
    EventBus,
    events::{BusEvent, NodeEvent, new_event_id, now_timestamp},
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::store::NodeStore;
use crate::types::{NodeInfo, NodeStatus, PairingToken};

/// Pairing state machine states.
pub enum PairingState {
    /// Token generated, waiting for peer to connect.
    AwaitingPeer {
        token: PairingToken,
        our_keypair: Arc<DeviceKeypair>,
    },
    /// Peer connected, public keys exchanged, awaiting signature verification.
    KeyExchange {
        our_keypair: Arc<DeviceKeypair>,
        peer_public: VerifyingKey,
    },
    /// Both sides display fingerprint for mutual confirmation.
    AwaitingConfirmation {
        our_keypair: Arc<DeviceKeypair>,
        peer_public: VerifyingKey,
        fingerprint: String,
    },
    /// Pairing complete, node info stored.
    Complete { node_info: NodeInfo },
    /// Pairing failed.
    Failed { reason: String },
}

/// Manages the pairing flow for this node.
pub struct PairingManager {
    keypair: Arc<DeviceKeypair>,
    store: Arc<NodeStore>,
    event_bus: Arc<EventBus>,
}

impl PairingManager {
    /// Create a new pairing manager.
    pub fn new(
        keypair: Arc<DeviceKeypair>,
        store: Arc<NodeStore>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            keypair,
            store,
            event_bus,
        }
    }

    /// Generate a new pairing token and render it as a QR code for the terminal.
    ///
    /// Returns the token and the QR code string for display.
    pub fn initiate_pairing(&self, host: &str, port: u16) -> (PairingToken, String) {
        let token = PairingToken::generate();
        let qr_content = render_pairing_qr(&token.value, host, port);
        info!("pairing token generated, expires in 15 minutes");
        (token, qr_content)
    }

    /// Validate an incoming pairing request token.
    ///
    /// Returns `true` if the token matches and is still valid.
    pub fn validate_token(&self, token: &mut PairingToken, incoming_token: &str) -> bool {
        if !token.is_valid() {
            warn!("pairing token expired or already used");
            return false;
        }
        if token.value != incoming_token {
            warn!("pairing token mismatch");
            return false;
        }
        token.consume();
        true
    }

    /// Verify the peer's public key by checking their signature over a challenge.
    ///
    /// The challenge is: `sort([our_pubkey, their_pubkey])` concatenated.
    /// This ensures both sides sign the same data.
    pub fn verify_peer_signature(
        &self,
        peer_public_bytes: &[u8; 32],
        peer_signature_bytes: &[u8; 64],
    ) -> Result<VerifyingKey, crate::NodeError> {
        let peer_public = VerifyingKey::from_bytes(peer_public_bytes)
            .map_err(|e| crate::NodeError::Auth(format!("invalid peer public key: {e}")))?;

        let challenge = build_challenge(&self.keypair.public_bytes(), peer_public_bytes);
        let signature = Signature::from_bytes(peer_signature_bytes);

        peer_public.verify(&challenge, &signature).map_err(|e| {
            crate::NodeError::Auth(format!("peer signature verification failed: {e}"))
        })?;

        debug!("peer signature verified successfully");
        Ok(peer_public)
    }

    /// Sign the challenge for the peer to verify us.
    ///
    /// Returns our signature bytes over the same deterministic challenge.
    pub fn sign_challenge(&self, peer_public_bytes: &[u8; 32]) -> Vec<u8> {
        let challenge = build_challenge(&self.keypair.public_bytes(), peer_public_bytes);
        let signature = self.keypair.sign(&challenge);
        signature.to_bytes().to_vec()
    }

    /// Compute the fingerprint for mutual confirmation display.
    pub fn compute_fingerprint(&self, peer_public_bytes: &[u8; 32]) -> String {
        compute_pairing_fingerprint(&self.keypair.public_bytes(), peer_public_bytes)
    }

    /// Complete the pairing: save to store and publish event.
    pub async fn complete_pairing(
        &self,
        peer_public_bytes: &[u8; 32],
        peer_name: &str,
        peer_endpoint: Option<String>,
        capabilities: Vec<crate::types::NodeCapability>,
    ) -> Result<NodeInfo, crate::NodeError> {
        let node_id = format!("node-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let public_key_hex = hex::encode(peer_public_bytes);
        let now = chrono::Utc::now().to_rfc3339();

        let info = NodeInfo {
            node_id: node_id.clone(),
            name: peer_name.to_string(),
            public_key_hex,
            capabilities,
            endpoint: peer_endpoint,
            paired_at: now,
            last_seen: None,
            status: NodeStatus::Online,
            battery_percent: None,
            memory_used_mb: None,
            memory_total_mb: None,
        };

        self.store.save_pairing(&info).await?;

        self.event_bus
            .publish(BusEvent::Node(NodeEvent::Paired {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                node_id: node_id.clone(),
                name: peer_name.to_string(),
            }))
            .await;

        info!(node_id = %node_id, name = %peer_name, "pairing completed");
        Ok(info)
    }

    /// Publish a pairing failure event.
    pub async fn publish_failure(&self, reason: &str) {
        warn!(reason = %reason, "pairing failed");
        self.event_bus
            .publish(BusEvent::Node(NodeEvent::PairingFailed {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                reason: reason.to_string(),
            }))
            .await;
    }

    /// Get a reference to this node's keypair.
    pub fn keypair(&self) -> &DeviceKeypair {
        &self.keypair
    }
}

/// Build a deterministic challenge from two public keys.
///
/// Sort the keys so the challenge is order-independent (both sides compute the same value).
fn build_challenge(key_a: &[u8; 32], key_b: &[u8; 32]) -> Vec<u8> {
    let (first, second) = if key_a < key_b {
        (key_a, key_b)
    } else {
        (key_b, key_a)
    };
    let mut challenge = Vec::with_capacity(64);
    challenge.extend_from_slice(first);
    challenge.extend_from_slice(second);
    challenge
}

/// Compute a deterministic fingerprint from two public keys.
///
/// Both sides compute the same fingerprint regardless of who initiated.
/// Format: ABCD-EFGH-IJKL-MNOP (4 groups of 4 hex chars from SHA-256 hash).
pub fn compute_pairing_fingerprint(key_a: &[u8; 32], key_b: &[u8; 32]) -> String {
    let (first, second) = if key_a < key_b {
        (key_a, key_b)
    } else {
        (key_b, key_a)
    };

    let mut hasher = Sha256::new();
    hasher.update(first);
    hasher.update(second);
    let hash = hasher.finalize();

    let hex_str = hex::encode(&hash[..8]);
    format!(
        "{}-{}-{}-{}",
        &hex_str[0..4],
        &hex_str[4..8],
        &hex_str[8..12],
        &hex_str[12..16]
    )
}

/// Render a pairing URI as a QR code for terminal display.
///
/// Uses Dense1x2 (Unicode half-block characters) for compact rendering
/// with inverted colors for readability on dark terminals.
pub fn render_pairing_qr(token: &str, host: &str, port: u16) -> String {
    use qrcode::QrCode;
    use qrcode::render::unicode::Dense1x2;

    let uri = format!("blufio-pair://{}:{}?token={}", host, port, token);
    match QrCode::new(uri.as_bytes()) {
        Ok(code) => code
            .render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
        Err(e) => {
            warn!("QR code generation failed: {e}, falling back to token display");
            format!("Pairing token: {token}\nConnect to: ws://{host}:{port}/nodes/pair")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_deterministic_regardless_of_order() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];

        let fp_ab = compute_pairing_fingerprint(&key_a, &key_b);
        let fp_ba = compute_pairing_fingerprint(&key_b, &key_a);

        assert_eq!(fp_ab, fp_ba);
    }

    #[test]
    fn fingerprint_format_is_four_groups() {
        let key_a = [0xAA; 32];
        let key_b = [0xBB; 32];

        let fp = compute_pairing_fingerprint(&key_a, &key_b);
        let parts: Vec<&str> = fp.split('-').collect();

        assert_eq!(parts.len(), 4);
        for part in &parts {
            assert_eq!(part.len(), 4);
            assert!(part.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn pairing_token_validity() {
        let token = PairingToken::generate();
        assert!(token.is_valid());
        assert_eq!(token.value.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn pairing_token_consume() {
        let mut token = PairingToken::generate();
        assert!(token.is_valid());
        token.consume();
        assert!(!token.is_valid());
    }

    #[test]
    fn challenge_is_order_independent() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];

        let c_ab = build_challenge(&key_a, &key_b);
        let c_ba = build_challenge(&key_b, &key_a);

        assert_eq!(c_ab, c_ba);
    }

    #[test]
    fn qr_code_renders_without_panic() {
        let qr = render_pairing_qr("deadbeef", "localhost", 9877);
        assert!(!qr.is_empty());
    }
}
