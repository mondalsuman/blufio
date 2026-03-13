// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the iMessage channel adapter.
//!
//! Uses wiremock to mock the BlueBubbles REST API and tests message
//! sending, webhook parsing, and error handling.

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use blufio_imessage::api::BlueBubblesClient;
use blufio_imessage::types::{BlueBubblesMessage, BlueBubblesWebhookPayload};

// ---------------------------------------------------------------------------
// BlueBubbles REST API: GET /api/v1/server/info
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_info_returns_valid_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/server/info"))
        .and(query_param("password", "test-pass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "os_version": "15.0",
            "server_version": "1.5.0",
            "private_api": true
        })))
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "test-pass");
    let info = client.server_info().await.expect("server_info should succeed");
    assert_eq!(info.os_version.as_deref(), Some("15.0"));
    assert_eq!(info.server_version.as_deref(), Some("1.5.0"));
    assert_eq!(info.private_api, Some(true));
}

// ---------------------------------------------------------------------------
// POST /api/v1/message/text: sends message, returns 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_message_returns_guid() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/message/text"))
        .and(query_param("password", "test-pass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": 200,
            "message": "Success",
            "data": {
                "guid": "iMessage;-;msg-guid-123"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "test-pass");
    let guid = client
        .send_message("iMessage;-;+1234567890", "Hello from Blufio!")
        .await
        .expect("send_message should succeed");

    assert_eq!(guid, "iMessage;-;msg-guid-123");
}

// ---------------------------------------------------------------------------
// Webhook parsing: incoming message JSON -> correct struct deserialization
// ---------------------------------------------------------------------------

#[test]
fn webhook_payload_deserializes_new_message() {
    let json = serde_json::json!({
        "type": "new-message",
        "data": {
            "guid": "msg-guid-456",
            "text": "Hello from user",
            "handle": {
                "address": "+1234567890"
            },
            "chatGuid": "iMessage;-;+1234567890",
            "isFromMe": false,
            "dateCreated": "2026-01-01T12:00:00Z",
            "associatedMessageType": 0
        }
    });

    let payload: BlueBubblesWebhookPayload =
        serde_json::from_value(json).expect("should deserialize webhook payload");
    assert_eq!(payload.type_field, "new-message");

    let message: BlueBubblesMessage =
        serde_json::from_value(payload.data).expect("should deserialize message data");
    assert_eq!(message.guid, "msg-guid-456");
    assert_eq!(message.text.as_deref(), Some("Hello from user"));
    assert!(!message.is_from_me);
    assert_eq!(
        message.chat_guid.as_deref(),
        Some("iMessage;-;+1234567890")
    );
    assert_eq!(message.associated_message_type, Some(0));
}

#[test]
fn webhook_payload_tapback_has_nonzero_associated_type() {
    let json = serde_json::json!({
        "type": "new-message",
        "data": {
            "guid": "tapback-guid",
            "text": null,
            "handle": { "address": "+1234567890" },
            "chatGuid": "iMessage;-;+1234567890",
            "isFromMe": false,
            "dateCreated": "2026-01-01T12:00:00Z",
            "associatedMessageType": 2000
        }
    });

    let payload: BlueBubblesWebhookPayload =
        serde_json::from_value(json).expect("should deserialize tapback");
    let message: BlueBubblesMessage =
        serde_json::from_value(payload.data).expect("should deserialize tapback message");
    assert_eq!(message.associated_message_type, Some(2000));
}

// ---------------------------------------------------------------------------
// Edge cases: API returns 401 (auth failure)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_info_401_auth_failure() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/server/info"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "wrong-pass");
    let result = client.server_info().await;
    assert!(result.is_err(), "should fail with 401");
}

// ---------------------------------------------------------------------------
// Edge cases: API returns 500 (server error, single retry)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_info_500_retries_once() {
    let mock_server = MockServer::start().await;

    // First request returns 500, second returns success.
    // wiremock serves in order when we use up_to_n_times.
    Mock::given(method("GET"))
        .and(path("/api/v1/server/info"))
        .and(query_param("password", "test-pass"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/server/info"))
        .and(query_param("password", "test-pass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "os_version": "15.0",
            "server_version": "1.5.0",
            "private_api": false
        })))
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "test-pass");
    let info = client.server_info().await.expect("should succeed after retry");
    assert_eq!(info.os_version.as_deref(), Some("15.0"));
}

// ---------------------------------------------------------------------------
// Edge cases: empty message list
// ---------------------------------------------------------------------------

#[test]
fn webhook_payload_empty_text() {
    let json = serde_json::json!({
        "type": "new-message",
        "data": {
            "guid": "empty-msg-guid",
            "text": "",
            "handle": { "address": "+1234567890" },
            "chatGuid": "iMessage;-;+1234567890",
            "isFromMe": false,
            "dateCreated": "2026-01-01T12:00:00Z",
            "associatedMessageType": 0
        }
    });

    let payload: BlueBubblesWebhookPayload =
        serde_json::from_value(json).expect("should deserialize empty text message");
    let message: BlueBubblesMessage =
        serde_json::from_value(payload.data).expect("should deserialize");
    assert_eq!(message.text.as_deref(), Some(""));
}

#[test]
fn webhook_payload_null_text() {
    let json = serde_json::json!({
        "type": "new-message",
        "data": {
            "guid": "null-text-guid",
            "text": null,
            "handle": { "address": "+1234567890" },
            "chatGuid": "iMessage;-;+1234567890",
            "isFromMe": false,
            "dateCreated": "2026-01-01T12:00:00Z",
            "associatedMessageType": 0
        }
    });

    let payload: BlueBubblesWebhookPayload =
        serde_json::from_value(json).expect("should deserialize null text");
    let message: BlueBubblesMessage =
        serde_json::from_value(payload.data).expect("should deserialize");
    assert!(message.text.is_none());
}

// ---------------------------------------------------------------------------
// Edge cases: malformed JSON response
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_info_malformed_json_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/server/info"))
        .and(query_param("password", "test-pass"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("not valid json {{{")
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "test-pass");
    let result = client.server_info().await;
    assert!(result.is_err(), "malformed JSON should produce an error");
}

// ---------------------------------------------------------------------------
// Query-param auth: password is correctly appended
// ---------------------------------------------------------------------------

#[tokio::test]
async fn password_appended_as_query_param() {
    let mock_server = MockServer::start().await;

    // The mock only matches if password query param is "my-secret-pass"
    Mock::given(method("GET"))
        .and(path("/api/v1/server/info"))
        .and(query_param("password", "my-secret-pass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "os_version": "15.0",
            "server_version": "1.5.0",
            "private_api": true
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "my-secret-pass");
    let result = client.server_info().await;
    assert!(result.is_ok(), "query-param auth should be accepted");
}

// ---------------------------------------------------------------------------
// Send message: client error (4xx)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_message_client_error_400() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/message/text"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "status": 400,
            "message": "Bad Request",
            "data": null
        })))
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "test-pass");
    let result = client.send_message("chat-guid", "hello").await;
    assert!(result.is_err(), "400 should produce error");
}

// ---------------------------------------------------------------------------
// Send message: response without guid falls back to generated UUID
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_message_no_guid_in_response_generates_fallback() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/message/text"))
        .and(query_param("password", "test-pass"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": 200,
            "message": "Success",
            "data": {}
        })))
        .mount(&mock_server)
        .await;

    let client = BlueBubblesClient::new(&mock_server.uri(), "test-pass");
    let guid = client
        .send_message("chat-guid", "test message")
        .await
        .expect("should succeed with fallback UUID");

    // Should be a valid UUID (36 chars with hyphens)
    assert!(!guid.is_empty(), "fallback GUID should not be empty");
}

#[test]
fn webhook_self_sent_message_flagged() {
    let json = serde_json::json!({
        "type": "new-message",
        "data": {
            "guid": "self-msg-guid",
            "text": "Hello",
            "handle": { "address": "+1234567890" },
            "chatGuid": "iMessage;-;+1234567890",
            "isFromMe": true,
            "dateCreated": "2026-01-01T12:00:00Z",
            "associatedMessageType": 0
        }
    });

    let payload: BlueBubblesWebhookPayload =
        serde_json::from_value(json).expect("should deserialize self-sent message");
    let message: BlueBubblesMessage =
        serde_json::from_value(payload.data).expect("should deserialize");
    assert!(message.is_from_me, "self-sent messages should be flagged");
}
