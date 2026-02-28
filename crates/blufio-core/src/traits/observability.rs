// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Observability adapter trait for metrics and telemetry.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::MetricEvent;

/// Adapter for recording metrics, traces, and telemetry events.
///
/// Observability adapters enable monitoring of the agent's behavior,
/// performance, and resource consumption.
#[async_trait]
pub trait ObservabilityAdapter: PluginAdapter {
    /// Records a metric or telemetry event.
    async fn record(&self, event: MetricEvent) -> Result<(), BlufioError>;
}
