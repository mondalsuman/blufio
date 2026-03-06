// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Signal-cli JSON-RPC message types.

use serde::{Deserialize, Serialize};

/// Incoming JSON-RPC notification from signal-cli.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignalNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: SignalParams,
}

/// Parameters wrapping the signal envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignalParams {
    pub envelope: SignalEnvelope,
}

/// A signal-cli envelope containing message data.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalEnvelope {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_number: Option<String>,
    #[serde(default)]
    pub source_name: Option<String>,
    #[serde(default)]
    pub timestamp: Option<u64>,
    #[serde(default)]
    pub data_message: Option<SignalDataMessage>,
}

/// A Signal data message (text content).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDataMessage {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub timestamp: Option<u64>,
    #[serde(default)]
    pub group_info: Option<SignalGroupInfo>,
}

/// Group information for a group message.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalGroupInfo {
    pub group_id: String,
}

/// Outgoing JSON-RPC request to signal-cli.
#[derive(Debug, Clone, Serialize)]
pub struct SignalJsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: String,
}

/// Response from signal-cli JSON-RPC.
#[derive(Debug, Clone, Deserialize)]
pub struct SignalJsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_text_message_notification() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "receive",
            "params": {
                "envelope": {
                    "source": "+15551234567",
                    "sourceNumber": "+15551234567",
                    "sourceName": "Alice",
                    "timestamp": 1709000000000,
                    "dataMessage": {
                        "message": "Hello from Signal!",
                        "timestamp": 1709000000000
                    }
                }
            }
        }"#;

        let notif: SignalNotification = serde_json::from_str(json).unwrap();
        assert_eq!(notif.method, "receive");
        assert_eq!(
            notif.params.envelope.source_number.as_deref(),
            Some("+15551234567")
        );
        assert_eq!(
            notif
                .params
                .envelope
                .data_message
                .as_ref()
                .unwrap()
                .message
                .as_deref(),
            Some("Hello from Signal!")
        );
    }

    #[test]
    fn deserialize_group_message() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "receive",
            "params": {
                "envelope": {
                    "source": "+15551234567",
                    "sourceNumber": "+15551234567",
                    "sourceName": "Bob",
                    "timestamp": 1709000000000,
                    "dataMessage": {
                        "message": "Group hello",
                        "timestamp": 1709000000000,
                        "groupInfo": {
                            "groupId": "YWJjMTIz"
                        }
                    }
                }
            }
        }"#;

        let notif: SignalNotification = serde_json::from_str(json).unwrap();
        let dm = notif.params.envelope.data_message.unwrap();
        assert_eq!(dm.group_info.as_ref().unwrap().group_id, "YWJjMTIz");
    }
}
