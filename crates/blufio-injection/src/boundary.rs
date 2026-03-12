// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! L3 HMAC-SHA256 boundary token system for cryptographic content zone separation.
//!
//! Generates per-session HMAC keys via HKDF-SHA256 and uses them to sign
//! content zones (static, conditional, dynamic). Boundary tokens are verified
//! and stripped before the LLM sees the assembled context. Tampered or spoofed
//! zones are detected and removed, emitting [`SecurityEvent::BoundaryFailure`].
//!
//! Token format: `<<BLUF-ZONE-v1:{zone}:{source}:{hex_tag}>>`

use std::fmt;

use regex::Regex;
use ring::{hkdf, hmac};
use std::sync::LazyLock;

use crate::config::HmacBoundaryConfig;
use crate::events::boundary_failure_event;
use blufio_bus::events::SecurityEvent;

/// Regex pattern for parsing boundary tokens.
///
/// Captures: zone (word chars), source (non-greedy, may contain colons like "mcp:server"),
/// hex tag (64 hex digits).
/// The source field uses a non-greedy match followed by `:` and exactly 64 hex chars
/// to correctly handle sources containing colons (e.g., "mcp:weather_server").
static BOUNDARY_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<<BLUF-ZONE-v1:(\w+):(.+?):([0-9a-f]{64})>>").expect("boundary regex must compile")
});

/// Content zone types for the assembled context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoneType {
    /// System prompt and static instructions.
    Static,
    /// Conditional providers (memory, skills, trust zone, archives).
    Conditional,
    /// Dynamic conversation messages.
    Dynamic,
}

impl fmt::Display for ZoneType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZoneType::Static => write!(f, "static"),
            ZoneType::Conditional => write!(f, "conditional"),
            ZoneType::Dynamic => write!(f, "dynamic"),
        }
    }
}

impl ZoneType {
    /// Parse a zone type from its string representation.
    pub fn from_str_value(s: &str) -> Option<ZoneType> {
        match s {
            "static" => Some(ZoneType::Static),
            "conditional" => Some(ZoneType::Conditional),
            "dynamic" => Some(ZoneType::Dynamic),
            _ => None,
        }
    }
}

/// A parsed boundary token extracted from assembled context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryToken {
    /// Version number (always 1 for v1 format).
    pub version: u8,
    /// Content zone type.
    pub zone: ZoneType,
    /// Source provenance: "system", "user", "mcp:{server_name}", "skill:{skill_name}".
    pub source: String,
    /// Hex-encoded HMAC-SHA256 tag (64 hex characters).
    pub tag: String,
}

impl fmt::Display for BoundaryToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<<BLUF-ZONE-v1:{}:{}:{}>>",
            self.zone, self.source, self.tag
        )
    }
}

impl BoundaryToken {
    /// Parse a boundary token from its string representation.
    pub fn parse(s: &str) -> Option<BoundaryToken> {
        let caps = BOUNDARY_TOKEN_RE.captures(s)?;
        let zone = ZoneType::from_str_value(caps.get(1)?.as_str())?;
        let source = caps.get(2)?.as_str().to_string();
        let tag = caps.get(3)?.as_str().to_string();
        Some(BoundaryToken {
            version: 1,
            zone,
            source,
            tag,
        })
    }
}

/// Content that has been signed with an HMAC boundary token.
#[derive(Debug, Clone)]
pub struct BoundedContent {
    /// Content zone type.
    pub zone: ZoneType,
    /// Source provenance metadata.
    pub source: String,
    /// The actual zone content (plaintext).
    pub content: String,
    /// The boundary token containing the HMAC tag.
    pub token: BoundaryToken,
}

/// Manages HMAC boundary token generation, validation, and stripping.
///
/// Each instance is tied to a session via HKDF-derived key.
/// When `enabled=false`, all methods pass through content unchanged.
pub struct BoundaryManager {
    /// Per-session HMAC signing key derived via HKDF.
    session_key: hmac::Key,
    /// Whether boundary token operations are active.
    enabled: bool,
}

impl BoundaryManager {
    /// Create a new boundary manager with a per-session HMAC key.
    ///
    /// Derives the session key via HKDF-SHA256:
    /// - Salt: `session_id` bytes
    /// - IKM: `master_key` (32-byte vault master key)
    /// - Info: `b"hmac-boundary"` context string
    ///
    /// If `config.enabled` is false, all operations become no-ops.
    pub fn new(master_key: &[u8; 32], session_id: &str, config: &HmacBoundaryConfig) -> Self {
        let session_key = derive_session_key(master_key, session_id);
        Self {
            session_key,
            enabled: config.enabled,
        }
    }

    /// Sign a content zone and produce a [`BoundedContent`].
    ///
    /// Computes HMAC-SHA256(session_key, content) and hex-encodes the tag.
    /// When disabled, returns a `BoundedContent` with an empty tag.
    pub fn sign_zone(&self, zone: ZoneType, source: &str, content: &str) -> BoundedContent {
        if !self.enabled {
            return BoundedContent {
                zone,
                source: source.to_string(),
                content: content.to_string(),
                token: BoundaryToken {
                    version: 1,
                    zone,
                    source: source.to_string(),
                    tag: String::new(),
                },
            };
        }

        let tag = hmac::sign(&self.session_key, content.as_bytes());
        let hex_tag = hex::encode(tag.as_ref());

        let token = BoundaryToken {
            version: 1,
            zone,
            source: source.to_string(),
            tag: hex_tag,
        };

        BoundedContent {
            zone,
            source: source.to_string(),
            content: content.to_string(),
            token,
        }
    }

    /// Verify a bounded content's HMAC tag against its content.
    ///
    /// Uses `ring::hmac::verify` for constant-time (timing-safe) comparison.
    /// Returns `true` if the content has not been tampered with.
    /// When disabled, always returns `true`.
    pub fn verify_zone(&self, bounded: &BoundedContent) -> bool {
        if !self.enabled {
            return true;
        }

        let tag_bytes = match hex::decode(&bounded.token.tag) {
            Ok(b) => b,
            Err(_) => return false,
        };

        // ring::hmac::verify uses constant-time comparison -- NEVER use == on HMAC tags.
        hmac::verify(&self.session_key, bounded.content.as_bytes(), &tag_bytes).is_ok()
    }

    /// Wrap content with start and end boundary tokens.
    ///
    /// Returns: `{token}\n{content}\n{token}`
    /// When disabled, returns content unchanged.
    pub fn wrap_content(&self, zone: ZoneType, source: &str, content: &str) -> String {
        if !self.enabled {
            return content.to_string();
        }

        let bounded = self.sign_zone(zone, source, content);
        let token_str = bounded.token.to_string();
        format!("{}\n{}\n{}", token_str, content, token_str)
    }

    /// Validate all boundary-marked zones in assembled context and strip tokens.
    ///
    /// Parses all `<<BLUF-ZONE-v1:...>>` tokens, extracts content between pairs,
    /// verifies HMAC for each zone. Valid zones have their content preserved (tokens
    /// stripped). Invalid zones are removed entirely and a `SecurityEvent::BoundaryFailure`
    /// is generated for each.
    ///
    /// Returns (clean_text, failure_events).
    /// When disabled, strips all tokens and returns content with no failures.
    pub fn validate_and_strip(
        &self,
        assembled_context: &str,
        correlation_id: &str,
    ) -> (String, Vec<SecurityEvent>) {
        if !self.enabled {
            return (Self::strip_all_tokens(assembled_context), Vec::new());
        }

        let mut failures: Vec<SecurityEvent> = Vec::new();
        let mut result = String::new();

        // Find all token positions
        let token_matches: Vec<_> = BOUNDARY_TOKEN_RE.find_iter(assembled_context).collect();

        if token_matches.is_empty() {
            // No tokens found -- return text as-is
            return (assembled_context.to_string(), failures);
        }

        // Process pairs of tokens (start, end)
        let mut i = 0;
        let mut last_end = 0;

        while i + 1 < token_matches.len() {
            let start_match = &token_matches[i];
            let end_match = &token_matches[i + 1];

            // Parse both tokens
            let start_token = BoundaryToken::parse(start_match.as_str());
            let end_token = BoundaryToken::parse(end_match.as_str());

            match (start_token, end_token) {
                (Some(st), Some(et)) if st == et => {
                    // Matching pair -- extract content between them
                    // Add any text before the start token
                    let before = &assembled_context[last_end..start_match.start()];
                    if !before.is_empty() {
                        result.push_str(before);
                    }

                    // Content between tokens (strip leading/trailing newline)
                    let between = &assembled_context[start_match.end()..end_match.start()];
                    let content = between.strip_prefix('\n').unwrap_or(between);
                    let content = content.strip_suffix('\n').unwrap_or(content);

                    // Verify HMAC
                    let bounded = BoundedContent {
                        zone: st.zone,
                        source: st.source.clone(),
                        content: content.to_string(),
                        token: st.clone(),
                    };

                    if self.verify_zone(&bounded) {
                        // Valid: include content
                        result.push_str(content);
                    } else {
                        // Invalid: strip content, emit failure event
                        tracing::warn!(
                            zone = %bounded.zone,
                            source = %bounded.source,
                            "HMAC boundary validation failed, stripping zone content"
                        );
                        failures.push(boundary_failure_event(
                            correlation_id,
                            &bounded.zone.to_string(),
                            &bounded.source,
                            "stripped",
                            content,
                        ));
                    }

                    last_end = end_match.end();
                    i += 2;
                }
                _ => {
                    // Non-matching or unparseable tokens -- skip this one
                    i += 1;
                }
            }
        }

        // Add any remaining text after the last processed token pair
        if last_end < assembled_context.len() {
            let remaining = &assembled_context[last_end..];
            result.push_str(&Self::strip_all_tokens(remaining));
        }

        (result, failures)
    }

    /// Generate fresh boundary tokens for new content (post-compaction/truncation).
    ///
    /// Use when content has been modified (e.g., after compaction or truncation)
    /// and needs new HMAC signatures.
    /// When disabled, returns content unchanged.
    pub fn re_sign(&self, zone: ZoneType, source: &str, new_content: &str) -> String {
        self.wrap_content(zone, source, new_content)
    }

    /// Strip all boundary tokens from text (static method).
    ///
    /// Removes all `<<BLUF-ZONE-v1:...>>` patterns via regex.
    /// Does not validate -- just removes token markers.
    pub fn strip_all_tokens(text: &str) -> String {
        BOUNDARY_TOKEN_RE.replace_all(text, "").to_string()
    }
}

/// Derive a per-session HMAC signing key using HKDF-SHA256.
///
/// - Salt: `session_id` bytes (domain separation per session)
/// - IKM: `master_key` (32-byte vault master key)
/// - Info: `b"hmac-boundary"` (application context string)
///
/// The derived key is deterministic: same inputs always produce the same key.
/// Different session IDs produce cryptographically independent keys.
fn derive_session_key(master_key: &[u8; 32], session_id: &str) -> hmac::Key {
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, session_id.as_bytes());
    let prk = salt.extract(master_key);
    let okm = prk
        .expand(&[b"hmac-boundary"], hmac::HMAC_SHA256)
        .expect("HMAC key length is valid for HKDF");
    let mut key_bytes = [0u8; 32];
    okm.fill(&mut key_bytes)
        .expect("32 bytes fits HMAC-SHA256 key");
    hmac::Key::new(hmac::HMAC_SHA256, &key_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(enabled: bool) -> HmacBoundaryConfig {
        HmacBoundaryConfig { enabled }
    }

    fn test_master_key() -> [u8; 32] {
        [0xAB; 32]
    }

    // --- Key derivation tests ---

    #[test]
    fn derive_session_key_deterministic() {
        let key = &test_master_key();
        let k1 = derive_session_key(key, "session-1");
        let k2 = derive_session_key(key, "session-1");
        // Sign the same content with both keys -- must produce identical tags
        let tag1 = hmac::sign(&k1, b"test");
        let tag2 = hmac::sign(&k2, b"test");
        assert_eq!(tag1.as_ref(), tag2.as_ref());
    }

    #[test]
    fn derive_session_key_different_sessions_produce_different_keys() {
        let key = &test_master_key();
        let k1 = derive_session_key(key, "session-1");
        let k2 = derive_session_key(key, "session-2");
        let tag1 = hmac::sign(&k1, b"test");
        let tag2 = hmac::sign(&k2, b"test");
        assert_ne!(tag1.as_ref(), tag2.as_ref());
    }

    // --- Sign zone tests ---

    #[test]
    fn sign_zone_produces_correct_token_format() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let bounded = mgr.sign_zone(ZoneType::Static, "system", "hello");
        let token_str = bounded.token.to_string();
        assert!(token_str.starts_with("<<BLUF-ZONE-v1:static:system:"));
        assert!(token_str.ends_with(">>"));
        // Hex tag should be 64 characters (32 bytes * 2)
        assert_eq!(bounded.token.tag.len(), 64);
        // Verify it matches the regex
        assert!(BOUNDARY_TOKEN_RE.is_match(&token_str));
    }

    // --- Verify zone tests ---

    #[test]
    fn verify_zone_returns_true_for_unmodified_content() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let bounded = mgr.sign_zone(ZoneType::Dynamic, "user", "hello world");
        assert!(mgr.verify_zone(&bounded));
    }

    #[test]
    fn verify_zone_returns_false_for_tampered_content() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let mut bounded = mgr.sign_zone(ZoneType::Dynamic, "user", "hello world");
        // Tamper with content (1-byte change)
        bounded.content = "hello worlD".to_string();
        assert!(!mgr.verify_zone(&bounded));
    }

    #[test]
    fn verify_zone_returns_false_for_different_session_key() {
        let mgr1 = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let mgr2 = BoundaryManager::new(&test_master_key(), "sess-2", &test_config(true));
        let bounded = mgr1.sign_zone(ZoneType::Static, "system", "secret stuff");
        // Verify with different session key should fail
        assert!(!mgr2.verify_zone(&bounded));
    }

    // --- Wrap content tests ---

    #[test]
    fn wrap_content_wraps_with_start_and_end_tokens() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let wrapped = mgr.wrap_content(ZoneType::Conditional, "mcp:weather", "forecast data");
        let lines: Vec<&str> = wrapped.lines().collect();
        assert_eq!(lines.len(), 3, "wrapped content should have 3 lines");
        // First and last lines should be identical tokens
        assert_eq!(lines[0], lines[2]);
        // Middle line is the content
        assert_eq!(lines[1], "forecast data");
        // Tokens should match the format
        assert!(BOUNDARY_TOKEN_RE.is_match(lines[0]));
    }

    // --- Strip boundaries tests ---

    #[test]
    fn strip_boundaries_removes_all_tokens() {
        let text = "<<BLUF-ZONE-v1:static:system:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa>>hello<<BLUF-ZONE-v1:static:system:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa>>";
        let stripped = BoundaryManager::strip_all_tokens(text);
        assert_eq!(stripped, "hello");
    }

    #[test]
    fn strip_boundaries_no_tokens_returns_unchanged() {
        let text = "just normal text without any tokens";
        let stripped = BoundaryManager::strip_all_tokens(text);
        assert_eq!(stripped, text);
    }

    // --- validate_and_strip tests ---

    #[test]
    fn validate_and_strip_returns_clean_content_for_valid_zones() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let wrapped = mgr.wrap_content(ZoneType::Static, "system", "system prompt");
        let (clean, failures) = mgr.validate_and_strip(&wrapped, "corr-1");
        assert_eq!(clean, "system prompt");
        assert!(failures.is_empty());
    }

    #[test]
    fn validate_and_strip_returns_failure_for_tampered_zones() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let wrapped = mgr.wrap_content(ZoneType::Dynamic, "user", "original content");
        // Tamper with content between tokens
        let tampered = wrapped.replace("original content", "TAMPERED content");
        let (clean, failures) = mgr.validate_and_strip(&tampered, "corr-2");
        // Tampered content should be stripped
        assert!(!clean.contains("TAMPERED"));
        assert_eq!(failures.len(), 1);
        match &failures[0] {
            SecurityEvent::BoundaryFailure {
                zone,
                source,
                action,
                content,
                ..
            } => {
                assert_eq!(zone, "dynamic");
                assert_eq!(source, "user");
                assert_eq!(action, "stripped");
                assert_eq!(content, "TAMPERED content");
            }
            _ => panic!("expected BoundaryFailure"),
        }
    }

    #[test]
    fn validate_and_strip_all_zones_tampered_returns_empty() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let w1 = mgr.wrap_content(ZoneType::Static, "system", "sys content");
        let w2 = mgr.wrap_content(ZoneType::Dynamic, "user", "user content");
        let assembled = format!("{}\n{}", w1, w2);
        // Tamper both zones
        let tampered = assembled
            .replace("sys content", "EVIL sys")
            .replace("user content", "EVIL user");
        let (clean, failures) = mgr.validate_and_strip(&tampered, "corr-3");
        // All content stripped
        assert!(!clean.contains("EVIL"));
        assert_eq!(failures.len(), 2);
    }

    // --- re_sign tests ---

    #[test]
    fn re_sign_produces_valid_boundaries() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let re_signed = mgr.re_sign(ZoneType::Dynamic, "user", "compacted summary");
        // Should be valid wrapped content
        let (clean, failures) = mgr.validate_and_strip(&re_signed, "corr-4");
        assert_eq!(clean, "compacted summary");
        assert!(failures.is_empty());
    }

    // --- ZoneType tests ---

    #[test]
    fn zone_type_covers_all_variants() {
        assert_eq!(ZoneType::Static.to_string(), "static");
        assert_eq!(ZoneType::Conditional.to_string(), "conditional");
        assert_eq!(ZoneType::Dynamic.to_string(), "dynamic");
    }

    #[test]
    fn zone_type_from_str_value_roundtrip() {
        for zone in &[ZoneType::Static, ZoneType::Conditional, ZoneType::Dynamic] {
            let s = zone.to_string();
            let parsed = ZoneType::from_str_value(&s).expect("should parse");
            assert_eq!(*zone, parsed);
        }
        assert!(ZoneType::from_str_value("unknown").is_none());
    }

    // --- Source provenance tests ---

    #[test]
    fn bounded_content_preserves_source_provenance() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));

        // Test various source provenances
        let sources = vec!["user", "mcp:weather_server", "skill:code_runner", "system"];
        for source in sources {
            let bounded = mgr.sign_zone(ZoneType::Conditional, source, "test content");
            assert_eq!(bounded.source, source);
            assert_eq!(bounded.token.source, source);
            // Token string contains the source
            let token_str = bounded.token.to_string();
            assert!(
                token_str.contains(source),
                "token should contain source: {}",
                source
            );
        }
    }

    // --- Disabled mode tests ---

    #[test]
    fn disabled_mode_sign_returns_content() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(false));
        let bounded = mgr.sign_zone(ZoneType::Static, "system", "hello");
        assert_eq!(bounded.content, "hello");
        assert!(bounded.token.tag.is_empty());
    }

    #[test]
    fn disabled_mode_verify_always_true() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(false));
        let bounded = BoundedContent {
            zone: ZoneType::Dynamic,
            source: "user".to_string(),
            content: "anything".to_string(),
            token: BoundaryToken {
                version: 1,
                zone: ZoneType::Dynamic,
                source: "user".to_string(),
                tag: "invalid_tag".to_string(),
            },
        };
        assert!(mgr.verify_zone(&bounded));
    }

    #[test]
    fn disabled_mode_wrap_returns_content_unchanged() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(false));
        let wrapped = mgr.wrap_content(ZoneType::Static, "system", "pass through");
        assert_eq!(wrapped, "pass through");
    }

    #[test]
    fn disabled_mode_validate_and_strip_returns_all_content() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(false));
        let text = "some <<BLUF-ZONE-v1:static:system:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa>> content";
        let (clean, failures) = mgr.validate_and_strip(text, "corr-5");
        assert!(!clean.contains("BLUF-ZONE"));
        assert!(failures.is_empty());
    }

    // --- BoundaryToken parsing tests ---

    #[test]
    fn boundary_token_parse_valid() {
        let token_str = "<<BLUF-ZONE-v1:static:system:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa>>";
        let token = BoundaryToken::parse(token_str).expect("should parse");
        assert_eq!(token.version, 1);
        assert_eq!(token.zone, ZoneType::Static);
        assert_eq!(token.source, "system");
        assert_eq!(token.tag.len(), 64);
    }

    #[test]
    fn boundary_token_parse_invalid_returns_none() {
        assert!(BoundaryToken::parse("not a token").is_none());
        assert!(BoundaryToken::parse("<<BLUF-ZONE-v2:static:system:aa>>").is_none());
        // Short hex tag
        assert!(BoundaryToken::parse("<<BLUF-ZONE-v1:static:system:aabb>>").is_none());
    }

    #[test]
    fn boundary_token_display_roundtrip() {
        let token = BoundaryToken {
            version: 1,
            zone: ZoneType::Dynamic,
            source: "user".to_string(),
            tag: "a".repeat(64),
        };
        let s = token.to_string();
        let parsed = BoundaryToken::parse(&s).expect("should parse");
        assert_eq!(token, parsed);
    }

    // --- Multi-zone validate_and_strip tests ---

    #[test]
    fn validate_and_strip_multiple_valid_zones() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let w1 = mgr.wrap_content(ZoneType::Static, "system", "system prompt here");
        let w2 = mgr.wrap_content(ZoneType::Conditional, "mcp:weather", "weather data");
        let w3 = mgr.wrap_content(ZoneType::Dynamic, "user", "user message");
        let assembled = format!("{}\n{}\n{}", w1, w2, w3);
        let (clean, failures) = mgr.validate_and_strip(&assembled, "corr-6");
        assert!(failures.is_empty());
        assert!(clean.contains("system prompt here"));
        assert!(clean.contains("weather data"));
        assert!(clean.contains("user message"));
        assert!(!clean.contains("BLUF-ZONE"));
    }

    #[test]
    fn validate_and_strip_mixed_valid_and_tampered() {
        let mgr = BoundaryManager::new(&test_master_key(), "sess-1", &test_config(true));
        let w1 = mgr.wrap_content(ZoneType::Static, "system", "valid system");
        let w2 = mgr.wrap_content(ZoneType::Dynamic, "user", "original user msg");
        let assembled = format!("{}\n{}", w1, w2);
        // Tamper only the user zone
        let tampered = assembled.replace("original user msg", "INJECTED content");
        let (clean, failures) = mgr.validate_and_strip(&tampered, "corr-7");
        // Valid system zone preserved
        assert!(clean.contains("valid system"));
        // Tampered user zone stripped
        assert!(!clean.contains("INJECTED"));
        assert_eq!(failures.len(), 1);
    }
}
