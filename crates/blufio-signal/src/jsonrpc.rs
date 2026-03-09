// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! JSON-RPC 2.0 client over TCP or Unix socket for signal-cli daemon.

use std::sync::atomic::{AtomicU64, Ordering};

use blufio_config::model::SignalConfig;
use blufio_core::error::BlufioError;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, warn};

use crate::types::{SignalJsonRpcRequest, SignalJsonRpcResponse, SignalNotification};

/// JSON-RPC client connected to signal-cli daemon.
pub struct JsonRpcClient {
    reader: BufReader<Box<dyn tokio::io::AsyncRead + Unpin + Send>>,
    writer: Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    request_counter: AtomicU64,
}

impl JsonRpcClient {
    /// Connect to signal-cli daemon.
    ///
    /// Auto-detects transport: Unix socket if `socket_path` is set, else TCP.
    pub async fn connect(config: &SignalConfig) -> Result<Self, BlufioError> {
        if let Some(ref socket_path) = config.socket_path {
            // Unix socket transport.
            #[cfg(unix)]
            {
                let stream = tokio::net::UnixStream::connect(socket_path)
                    .await
                    .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;
                let (read, write) = stream.into_split();
                Ok(Self {
                    reader: BufReader::new(Box::new(read)),
                    writer: Box::new(write),
                    request_counter: AtomicU64::new(1),
                })
            }
            #[cfg(not(unix))]
            {
                let _ = socket_path;
                Err(BlufioError::Config(
                    "Unix sockets are not supported on this platform".into(),
                ))
            }
        } else {
            // TCP transport.
            let host = config.host.as_deref().unwrap_or("127.0.0.1");
            let port = config.port.unwrap_or(7583);
            let addr = format!("{host}:{port}");

            let stream = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;
            let (read, write) = stream.into_split();
            Ok(Self {
                reader: BufReader::new(Box::new(read)),
                writer: Box::new(write),
                request_counter: AtomicU64::new(1),
            })
        }
    }

    /// Send a JSON-RPC request and read the response.
    pub async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<SignalJsonRpcResponse, BlufioError> {
        let id = self.request_counter.fetch_add(1, Ordering::Relaxed);
        let request = SignalJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: format!("req-{id}"),
        };

        let mut line = serde_json::to_string(&request)
            .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;
        line.push('\n');

        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;
        self.writer
            .flush()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;

        // Read response line.
        let mut response_line = String::new();
        self.reader
            .read_line(&mut response_line)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;

        serde_json::from_str(&response_line)
            .map_err(|e| BlufioError::channel_delivery_failed("signal", e))
    }

    /// Read the next notification from signal-cli.
    ///
    /// Returns `None` on EOF (connection closed).
    pub async fn read_notification(&mut self) -> Result<Option<SignalNotification>, BlufioError> {
        let mut line = String::new();
        let bytes = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("signal", e))?;

        if bytes == 0 {
            return Ok(None); // EOF
        }

        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }

        match serde_json::from_str::<SignalNotification>(line) {
            Ok(notif) => {
                if notif.method != "receive" {
                    debug!(method = %notif.method, "skipping non-receive signal-cli notification");
                    // Return an "empty" notification by recursing — in practice, skip.
                    return Ok(None);
                }
                Ok(Some(notif))
            }
            Err(e) => {
                warn!(error = %e, "failed to parse signal-cli notification, skipping");
                Ok(None)
            }
        }
    }
}
