// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SASL PLAIN authentication for IRC connections.
//!
//! Implements the SASL PLAIN mechanism manually using raw IRC commands.
//! This is preferred over NickServ IDENTIFY for modern IRC servers as it
//! authenticates before the connection is fully registered.

use base64::Engine;
use blufio_core::error::BlufioError;
use irc::proto::Command;
use tracing::debug;

/// Request SASL capability from the IRC server.
///
/// Sends `CAP REQ :sasl`. The actual CAP ACK handling happens in the message
/// stream processing in `lib.rs`.
pub async fn request_sasl_cap(client: &irc::client::Client) -> Result<(), BlufioError> {
    debug!("requesting SASL capability");
    client
        .send(Command::CAP(
            None,
            irc::proto::CapSubCommand::REQ,
            None,
            Some("sasl".into()),
        ))
        .map_err(|e| BlufioError::Channel {
            message: format!("failed to send CAP REQ :sasl: {e}"),
            source: Some(Box::new(e)),
        })
}

/// Encode SASL PLAIN credentials.
///
/// The SASL PLAIN format is: `\0{authcid}\0{password}` encoded as base64.
/// For IRC, authcid is typically the nickname.
pub fn encode_sasl_plain(nickname: &str, password: &str) -> String {
    let payload = format!("\0{nickname}\0{password}");
    base64::engine::general_purpose::STANDARD.encode(payload.as_bytes())
}

/// Send the AUTHENTICATE message with the encoded credentials.
pub async fn send_authenticate(
    client: &irc::client::Client,
    encoded: &str,
) -> Result<(), BlufioError> {
    debug!("sending AUTHENTICATE with credentials");
    client
        .send(Command::Raw(
            "AUTHENTICATE".into(),
            vec![encoded.to_string()],
        ))
        .map_err(|e| BlufioError::Channel {
            message: format!("failed to send AUTHENTICATE: {e}"),
            source: Some(Box::new(e)),
        })
}

/// Send CAP END to finish capability negotiation.
pub async fn finish_cap(client: &irc::client::Client) -> Result<(), BlufioError> {
    debug!("sending CAP END");
    client
        .send(Command::CAP(
            None,
            irc::proto::CapSubCommand::END,
            None,
            None,
        ))
        .map_err(|e| BlufioError::Channel {
            message: format!("failed to send CAP END: {e}"),
            source: Some(Box::new(e)),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_sasl_plain_correct() {
        let encoded = encode_sasl_plain("bot", "secret123");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert_eq!(decoded_str, "\0bot\0secret123");
    }

    #[test]
    fn encode_sasl_plain_with_special_chars() {
        let encoded = encode_sasl_plain("myBot", "p@ss!w0rd");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert_eq!(decoded_str, "\0myBot\0p@ss!w0rd");
    }

    #[test]
    fn encode_sasl_plain_is_valid_base64() {
        let encoded = encode_sasl_plain("nick", "pass");
        // Should be decodable.
        assert!(
            base64::engine::general_purpose::STANDARD
                .decode(&encoded)
                .is_ok()
        );
    }
}
