// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ed25519 device keypair generation and token validation.

use blufio_core::BlufioError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

/// An Ed25519 device keypair for authentication.
///
/// Used for bearer token validation. The public key (hex-encoded) serves
/// as the bearer token. More sophisticated challenge-response auth is
/// deferred to v2.
pub struct DeviceKeypair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl DeviceKeypair {
    /// Generate a new random Ed25519 keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Reconstruct a keypair from private key bytes.
    pub fn from_bytes(private_bytes: &[u8; 32]) -> Result<Self, BlufioError> {
        let signing_key = SigningKey::from_bytes(private_bytes);
        let verifying_key = VerifyingKey::from(&signing_key);
        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Get the private key bytes (for vault storage).
    pub fn private_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Get the public key bytes.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Get the hex-encoded public key (used as bearer token).
    pub fn public_hex(&self) -> String {
        hex::encode(self.public_bytes())
    }

    /// Verify a bearer token against this keypair.
    ///
    /// Simple validation: token must equal hex-encoded public key.
    /// More sophisticated challenge-response is deferred to v2.
    pub fn verify_token(&self, token: &str) -> bool {
        let expected = self.public_hex();
        token == expected
    }

    /// Sign arbitrary bytes with this keypair's private key.
    ///
    /// Returns an Ed25519 signature over the provided message bytes.
    /// Used for signing inter-agent messages (SEC-07).
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify a signature against this keypair's public key using strict mode.
    ///
    /// Strict verification rejects weak public keys per ed25519-dalek security
    /// recommendations, preventing weak key forgery attacks.
    ///
    /// Returns `Ok(())` if the signature is valid, or `BlufioError::Security`
    /// if verification fails.
    pub fn verify_strict(
        &self,
        message: &[u8],
        signature: &Signature,
    ) -> Result<(), BlufioError> {
        self.verifying_key
            .verify_strict(message, signature)
            .map_err(|e| {
                BlufioError::Security(format!("Ed25519 signature verification failed: {e}"))
            })
    }

    /// Verify a signature against this keypair's public key (non-strict).
    ///
    /// Less strict than `verify_strict` -- permits weak public keys.
    /// Prefer `verify_strict` for inter-agent message verification.
    pub fn verify(
        &self,
        message: &[u8],
        signature: &Signature,
    ) -> Result<(), BlufioError> {
        self.verifying_key
            .verify(message, signature)
            .map_err(|e| {
                BlufioError::Security(format!("Ed25519 signature verification failed: {e}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_creates_valid_keypair() {
        let kp = DeviceKeypair::generate();
        assert_eq!(kp.private_bytes().len(), 32);
        assert_eq!(kp.public_bytes().len(), 32);
        assert_eq!(kp.public_hex().len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn from_bytes_roundtrip() {
        let kp1 = DeviceKeypair::generate();
        let private = kp1.private_bytes();

        let kp2 = DeviceKeypair::from_bytes(&private).unwrap();
        assert_eq!(kp1.public_bytes(), kp2.public_bytes());
        assert_eq!(kp1.private_bytes(), kp2.private_bytes());
    }

    #[test]
    fn verify_token_correct() {
        let kp = DeviceKeypair::generate();
        let token = kp.public_hex();
        assert!(kp.verify_token(&token));
    }

    #[test]
    fn verify_token_wrong() {
        let kp = DeviceKeypair::generate();
        assert!(!kp.verify_token("wrong-token"));
    }

    #[test]
    fn verify_token_empty() {
        let kp = DeviceKeypair::generate();
        assert!(!kp.verify_token(""));
    }

    #[test]
    fn different_keypairs_have_different_tokens() {
        let kp1 = DeviceKeypair::generate();
        let kp2 = DeviceKeypair::generate();
        assert_ne!(kp1.public_hex(), kp2.public_hex());
    }

    #[test]
    fn sign_produces_64_byte_signature() {
        let kp = DeviceKeypair::generate();
        let sig = kp.sign(b"hello world");
        assert_eq!(sig.to_bytes().len(), 64);
    }

    #[test]
    fn verify_strict_succeeds_for_correct_signature() {
        let kp = DeviceKeypair::generate();
        let message = b"test payload";
        let sig = kp.sign(message);
        assert!(kp.verify_strict(message, &sig).is_ok());
    }

    #[test]
    fn verify_strict_fails_for_tampered_bytes() {
        let kp = DeviceKeypair::generate();
        let sig = kp.sign(b"original message");
        let result = kp.verify_strict(b"tampered message", &sig);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            BlufioError::Security(msg) => {
                assert!(msg.contains("Ed25519 signature verification failed"));
            }
            other => panic!("expected Security error, got: {other:?}"),
        }
    }

    #[test]
    fn verify_strict_fails_for_wrong_keypair() {
        let kp1 = DeviceKeypair::generate();
        let kp2 = DeviceKeypair::generate();
        let message = b"signed by kp1";
        let sig = kp1.sign(message);
        // Verify against kp2's public key should fail
        let result = kp2.verify_strict(message, &sig);
        assert!(result.is_err());
    }

    #[test]
    fn sign_verify_roundtrip_empty_bytes() {
        let kp = DeviceKeypair::generate();
        let message = b"";
        let sig = kp.sign(message);
        assert!(kp.verify_strict(message, &sig).is_ok());
    }

    #[test]
    fn sign_verify_roundtrip_small_payload() {
        let kp = DeviceKeypair::generate();
        let message = b"small";
        let sig = kp.sign(message);
        assert!(kp.verify_strict(message, &sig).is_ok());
    }

    #[test]
    fn sign_verify_roundtrip_large_payload() {
        let kp = DeviceKeypair::generate();
        let message = vec![0xABu8; 10 * 1024]; // 10KB
        let sig = kp.sign(&message);
        assert!(kp.verify_strict(&message, &sig).is_ok());
    }

    #[test]
    fn verify_non_strict_succeeds_for_correct_signature() {
        let kp = DeviceKeypair::generate();
        let message = b"test verify non-strict";
        let sig = kp.sign(message);
        assert!(kp.verify(message, &sig).is_ok());
    }

    #[test]
    fn verify_non_strict_fails_for_tampered_bytes() {
        let kp = DeviceKeypair::generate();
        let sig = kp.sign(b"original");
        let result = kp.verify(b"tampered", &sig);
        assert!(result.is_err());
    }
}
