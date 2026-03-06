// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ed25519 publisher keypair and cryptographic signing for WASM skills.
//!
//! [`PublisherKeypair`] provides Ed25519 signing and verification for skill
//! artifacts. This is distinct from the device keypair used for authentication
//! (`blufio-auth-keypair`) — publisher keys represent a skill author's identity.
//!
//! The signing workflow:
//! 1. Author generates a keypair: `blufio skill keygen`
//! 2. Author signs WASM: `blufio skill sign <wasm> <key>`
//! 3. User installs skill: `blufio skill install <wasm> <manifest>` (verifies .sig if present)
//! 4. Runtime verifies before every execution (hash + signature check)

use std::path::Path;

use blufio_core::BlufioError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

/// An Ed25519 publisher keypair for skill signing.
///
/// Separate from [`blufio_auth_keypair::DeviceKeypair`] — publisher keys
/// represent a skill author's identity, not a device's identity.
pub struct PublisherKeypair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl PublisherKeypair {
    /// Generate a new random Ed25519 publisher keypair.
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

    /// Get the private key bytes (for storage).
    pub fn private_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Get the public key bytes.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Get the hex-encoded public key (used as publisher_id).
    pub fn public_hex(&self) -> String {
        hex::encode(self.public_bytes())
    }

    /// Get the verifying key.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Sign data with this keypair.
    pub fn sign(&self, data: &[u8]) -> Signature {
        self.signing_key.sign(data)
    }

    /// Verify a signature against a public key.
    pub fn verify_signature(
        verifying_key: &VerifyingKey,
        data: &[u8],
        signature: &Signature,
    ) -> Result<(), BlufioError> {
        verifying_key
            .verify(data, signature)
            .map_err(|e| BlufioError::Security(format!("signature verification failed: {e}")))
    }
}

/// Compute a SHA-256 content hash of data, returned as a hex string (64 chars).
pub fn compute_content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Encode a signature as a hex string (128 chars for Ed25519).
pub fn signature_to_hex(sig: &Signature) -> String {
    hex::encode(sig.to_bytes())
}

/// Decode a hex-encoded signature.
pub fn signature_from_hex(hex_str: &str) -> Result<Signature, BlufioError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| BlufioError::Security(format!("invalid signature hex: {e}")))?;
    let byte_array: [u8; 64] = bytes
        .try_into()
        .map_err(|_| BlufioError::Security("signature must be exactly 64 bytes".to_string()))?;
    Ok(Signature::from_bytes(&byte_array))
}

// ---- File I/O for keypair persistence ----

const PRIVATE_KEY_HEADER: &str = "-----BEGIN BLUFIO PUBLISHER PRIVATE KEY-----";
const PRIVATE_KEY_FOOTER: &str = "-----END BLUFIO PUBLISHER PRIVATE KEY-----";
const PUBLIC_KEY_HEADER: &str = "-----BEGIN BLUFIO PUBLISHER PUBLIC KEY-----";
const PUBLIC_KEY_FOOTER: &str = "-----END BLUFIO PUBLISHER PUBLIC KEY-----";

/// Save a publisher keypair to private and public key files (PEM-like format).
pub fn save_keypair_to_file(
    keypair: &PublisherKeypair,
    private_path: &Path,
    public_path: &Path,
) -> Result<(), BlufioError> {
    let private_content = format!(
        "{}\n{}\n{}\n",
        PRIVATE_KEY_HEADER,
        hex::encode(keypair.private_bytes()),
        PRIVATE_KEY_FOOTER,
    );
    std::fs::write(private_path, &private_content).map_err(|e| BlufioError::Skill {
        message: format!(
            "failed to write private key to '{}': {e}",
            private_path.display()
        ),
        source: Some(Box::new(e)),
    })?;

    let public_content = format!(
        "{}\n{}\n{}\n",
        PUBLIC_KEY_HEADER,
        hex::encode(keypair.public_bytes()),
        PUBLIC_KEY_FOOTER,
    );
    std::fs::write(public_path, &public_content).map_err(|e| BlufioError::Skill {
        message: format!(
            "failed to write public key to '{}': {e}",
            public_path.display()
        ),
        source: Some(Box::new(e)),
    })?;

    Ok(())
}

/// Load a publisher keypair from a private key file (PEM-like format).
pub fn load_private_key_from_file(path: &Path) -> Result<PublisherKeypair, BlufioError> {
    let content = std::fs::read_to_string(path).map_err(|e| BlufioError::Skill {
        message: format!("failed to read private key from '{}': {e}", path.display()),
        source: Some(Box::new(e)),
    })?;

    let hex_line = extract_pem_content(&content, PRIVATE_KEY_HEADER, PRIVATE_KEY_FOOTER)?;
    let bytes = hex::decode(&hex_line).map_err(|e| {
        BlufioError::Security(format!(
            "invalid private key hex in '{}': {e}",
            path.display()
        ))
    })?;
    let byte_array: [u8; 32] = bytes.try_into().map_err(|_| {
        BlufioError::Security(format!(
            "private key must be exactly 32 bytes in '{}'",
            path.display()
        ))
    })?;
    PublisherKeypair::from_bytes(&byte_array)
}

/// Load a public key from a public key file (PEM-like format).
pub fn load_public_key_from_file(path: &Path) -> Result<VerifyingKey, BlufioError> {
    let content = std::fs::read_to_string(path).map_err(|e| BlufioError::Skill {
        message: format!("failed to read public key from '{}': {e}", path.display()),
        source: Some(Box::new(e)),
    })?;

    let hex_line = extract_pem_content(&content, PUBLIC_KEY_HEADER, PUBLIC_KEY_FOOTER)?;
    let bytes = hex::decode(&hex_line).map_err(|e| {
        BlufioError::Security(format!(
            "invalid public key hex in '{}': {e}",
            path.display()
        ))
    })?;
    let byte_array: [u8; 32] = bytes.try_into().map_err(|_| {
        BlufioError::Security(format!(
            "public key must be exactly 32 bytes in '{}'",
            path.display()
        ))
    })?;
    VerifyingKey::from_bytes(&byte_array).map_err(|e| {
        BlufioError::Security(format!(
            "invalid Ed25519 public key in '{}': {e}",
            path.display()
        ))
    })
}

/// Extract the hex content between PEM-like header and footer lines.
fn extract_pem_content(content: &str, header: &str, footer: &str) -> Result<String, BlufioError> {
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim() == header)
        .ok_or_else(|| BlufioError::Security(format!("missing header: {header}")))?;
    let end = lines
        .iter()
        .position(|l| l.trim() == footer)
        .ok_or_else(|| BlufioError::Security(format!("missing footer: {footer}")))?;

    if end <= start + 1 {
        return Err(BlufioError::Security(
            "no content between header and footer".to_string(),
        ));
    }

    // Collect all lines between header and footer (trimmed, joined).
    let hex_content: String = lines[start + 1..end]
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("");

    Ok(hex_content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generate_produces_valid_pair() {
        let kp = PublisherKeypair::generate();
        // Public key is derivable from private key.
        let reconstructed = PublisherKeypair::from_bytes(&kp.private_bytes()).unwrap();
        assert_eq!(kp.public_bytes(), reconstructed.public_bytes());
    }

    #[test]
    fn keypair_from_bytes_roundtrips() {
        let kp = PublisherKeypair::generate();
        let bytes = kp.private_bytes();
        let kp2 = PublisherKeypair::from_bytes(&bytes).unwrap();
        assert_eq!(kp.public_hex(), kp2.public_hex());
        assert_eq!(kp.private_bytes(), kp2.private_bytes());
    }

    #[test]
    fn sign_and_verify_succeeds() {
        let kp = PublisherKeypair::generate();
        let data = b"hello WASM skill";
        let sig = kp.sign(data);
        PublisherKeypair::verify_signature(kp.verifying_key(), data, &sig).unwrap();
    }

    #[test]
    fn verify_rejects_tampered_data() {
        let kp = PublisherKeypair::generate();
        let data = b"original data";
        let sig = kp.sign(data);
        let tampered = b"tampered data";
        let result = PublisherKeypair::verify_signature(kp.verifying_key(), tampered, &sig);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("signature verification failed"));
    }

    #[test]
    fn verify_rejects_wrong_public_key() {
        let kp1 = PublisherKeypair::generate();
        let kp2 = PublisherKeypair::generate();
        let data = b"some data";
        let sig = kp1.sign(data);
        let result = PublisherKeypair::verify_signature(kp2.verifying_key(), data, &sig);
        assert!(result.is_err());
    }

    #[test]
    fn compute_content_hash_is_consistent() {
        let data = b"test data for hashing";
        let h1 = compute_content_hash(data);
        let h2 = compute_content_hash(data);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn compute_content_hash_differs_for_different_input() {
        let h1 = compute_content_hash(b"data one");
        let h2 = compute_content_hash(b"data two");
        assert_ne!(h1, h2);
    }

    #[test]
    fn signature_hex_roundtrip() {
        let kp = PublisherKeypair::generate();
        let sig = kp.sign(b"test");
        let hex_str = signature_to_hex(&sig);
        assert_eq!(hex_str.len(), 128); // Ed25519 sig = 64 bytes = 128 hex chars
        let sig2 = signature_from_hex(&hex_str).unwrap();
        assert_eq!(sig.to_bytes(), sig2.to_bytes());
    }

    #[test]
    fn signature_from_hex_rejects_invalid() {
        assert!(signature_from_hex("not_hex").is_err());
        assert!(signature_from_hex("aabb").is_err()); // too short
    }

    #[test]
    fn keypair_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let private_path = dir.path().join("publisher.key");
        let public_path = dir.path().join("publisher.pub");

        let kp = PublisherKeypair::generate();
        save_keypair_to_file(&kp, &private_path, &public_path).unwrap();

        let kp2 = load_private_key_from_file(&private_path).unwrap();
        assert_eq!(kp.public_hex(), kp2.public_hex());
        assert_eq!(kp.private_bytes(), kp2.private_bytes());

        let pubkey = load_public_key_from_file(&public_path).unwrap();
        assert_eq!(pubkey.to_bytes(), kp.public_bytes());
    }

    #[test]
    fn load_private_key_rejects_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.key");

        // No header
        std::fs::write(&path, "just some random text").unwrap();
        assert!(load_private_key_from_file(&path).is_err());

        // Header but no content
        std::fs::write(
            &path,
            format!("{}\n{}\n", PRIVATE_KEY_HEADER, PRIVATE_KEY_FOOTER),
        )
        .unwrap();
        assert!(load_private_key_from_file(&path).is_err());

        // Header with wrong length hex
        std::fs::write(
            &path,
            format!("{}\naabb\n{}\n", PRIVATE_KEY_HEADER, PRIVATE_KEY_FOOTER),
        )
        .unwrap();
        assert!(load_private_key_from_file(&path).is_err());
    }

    #[test]
    fn public_hex_is_64_chars() {
        let kp = PublisherKeypair::generate();
        assert_eq!(kp.public_hex().len(), 64);
    }
}
