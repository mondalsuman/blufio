// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Twilio REST API client for outbound SMS.
//!
//! Uses raw `reqwest` calls with HTTP Basic auth (account_sid:auth_token)
//! rather than an SDK, as no mature official Twilio Rust SDK exists.

use blufio_core::error::BlufioError;
use tracing::warn;

use crate::types::{TwilioAccountInfo, TwilioSendResponse};

/// Client for the Twilio REST API.
pub struct TwilioClient {
    account_sid: String,
    auth_token: String,
    from_number: String,
    client: reqwest::Client,
}

impl TwilioClient {
    /// Create a new Twilio API client.
    ///
    /// Validates that `from_number` is in E.164 format.
    pub fn new(
        account_sid: &str,
        auth_token: &str,
        from_number: &str,
    ) -> Result<Self, BlufioError> {
        if !validate_e164(from_number) {
            return Err(BlufioError::Config(format!(
                "sms.twilio_phone_number must be E.164 format (e.g., +1234567890), got: {from_number}"
            )));
        }

        Ok(Self {
            account_sid: account_sid.to_string(),
            auth_token: auth_token.to_string(),
            from_number: from_number.to_string(),
            client: reqwest::Client::new(),
        })
    }

    /// Build the messages API URL for this account.
    fn messages_url(&self) -> String {
        format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        )
    }

    /// Build a form-urlencoded body for sending an SMS.
    fn build_form_body(to: &str, from: &str, body: &str) -> String {
        let params = [("To", to), ("From", from), ("Body", body)];
        serde_urlencoded::to_string(&params).unwrap_or_default()
    }

    /// Send an SMS via Twilio, returning the response.
    async fn do_send(
        &self,
        url: &str,
        to: &str,
        body: &str,
    ) -> Result<reqwest::Response, BlufioError> {
        let form_body = Self::build_form_body(to, &self.from_number, body);

        self.client
            .post(url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("sms", e))
    }

    /// Send an SMS message to the given phone number.
    ///
    /// Returns the Twilio message SID on success.
    pub async fn send_message(&self, to: &str, body: &str) -> Result<String, BlufioError> {
        let url = self.messages_url();

        let resp = self.do_send(&url, to, body).await?;
        let status = resp.status();

        // Handle 429 rate limiting: check Retry-After, wait, single retry.
        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);

            warn!(
                retry_after_secs = retry_after,
                "Twilio rate limited, retrying after delay"
            );

            tokio::time::sleep(std::time::Duration::from_secs(retry_after)).await;

            let retry_resp = self.do_send(&url, to, body).await?;

            if !retry_resp.status().is_success() {
                return Err(BlufioError::Config(format!(
                    "Twilio API error after retry: {}",
                    retry_resp.status()
                )));
            }

            let send_resp: TwilioSendResponse = retry_resp
                .json()
                .await
                .map_err(|e| BlufioError::channel_delivery_failed("sms", e))?;

            return Ok(send_resp
                .sid
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()));
        }

        if !status.is_success() {
            let body_text = resp
                .text()
                .await
                .unwrap_or_default();
            return Err(BlufioError::Config(format!(
                "Twilio API error ({status}): {body_text}"
            )));
        }

        let send_resp: TwilioSendResponse = resp
            .json()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("sms", e))?;

        Ok(send_resp
            .sid
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()))
    }

    /// Check account status (health check / credential verification).
    pub async fn account_status(&self) -> Result<String, BlufioError> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}.json",
            self.account_sid
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .send()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("sms", e))?;

        if !resp.status().is_success() {
            return Err(BlufioError::Config(format!(
                "Twilio account check failed: {}",
                resp.status()
            )));
        }

        let info: TwilioAccountInfo = resp
            .json()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("sms", e))?;

        Ok(info.status)
    }
}

/// Validate E.164 phone number format.
///
/// Must start with '+', remaining characters must be digits, minimum 8
/// characters total ('+' plus at least 7 digits).
pub fn validate_e164(number: &str) -> bool {
    if !number.starts_with('+') {
        return false;
    }

    let digits = &number[1..];
    if digits.is_empty() || digits.len() < 7 {
        return false;
    }

    digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_e164_valid() {
        assert!(validate_e164("+1234567890"));
    }

    #[test]
    fn test_validate_e164_valid_short() {
        // Minimum valid: +1234567 (7 digits)
        assert!(validate_e164("+1234567"));
    }

    #[test]
    fn test_validate_e164_missing_plus() {
        assert!(!validate_e164("1234567890"));
    }

    #[test]
    fn test_validate_e164_with_letters() {
        assert!(!validate_e164("+123abc"));
    }

    #[test]
    fn test_validate_e164_too_short() {
        assert!(!validate_e164("+12"));
    }

    #[test]
    fn test_validate_e164_empty() {
        assert!(!validate_e164(""));
    }

    #[test]
    fn test_validate_e164_only_plus() {
        assert!(!validate_e164("+"));
    }

    #[test]
    fn test_twilio_client_new_rejects_invalid_e164() {
        let result = TwilioClient::new("AC123", "token", "not-e164");
        assert!(result.is_err());
    }

    #[test]
    fn test_twilio_client_new_accepts_valid() {
        let result = TwilioClient::new("AC123", "token", "+1234567890");
        assert!(result.is_ok());
    }
}
