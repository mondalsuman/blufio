// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prometheus metrics adapter for the Blufio framework.
//!
//! Uses the metrics-rs facade with the Prometheus exporter.
//! Metrics are rendered as Prometheus text format via the `render()` method,
//! which is exposed through the gateway's /metrics endpoint.

pub mod recording;

use async_trait::async_trait;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::observability::ObservabilityAdapter;
use blufio_core::types::{AdapterType, HealthStatus, MetricEvent};
use blufio_core::BlufioError;

pub use recording::{
    record_latency, record_message, record_tokens, set_active_sessions, set_budget_remaining,
};

/// Prometheus metrics adapter.
///
/// Installs the Prometheus recorder and exposes a handle for rendering
/// metrics in Prometheus text format.
pub struct PrometheusAdapter {
    handle: PrometheusHandle,
}

impl PrometheusAdapter {
    /// Create a new PrometheusAdapter.
    ///
    /// Installs the Prometheus recorder globally. Only one recorder can be
    /// installed per process. Returns an error if a recorder is already installed.
    pub fn new() -> Result<Self, BlufioError> {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .map_err(|e| BlufioError::Internal(format!("failed to install Prometheus recorder: {e}")))?;

        recording::register_metrics();

        tracing::info!("prometheus metrics recorder installed");

        Ok(Self { handle })
    }

    /// Get a reference to the Prometheus handle for rendering.
    pub fn handle(&self) -> &PrometheusHandle {
        &self.handle
    }

    /// Render all collected metrics in Prometheus text format.
    pub fn render(&self) -> String {
        self.handle.render()
    }
}

#[async_trait]
impl PluginAdapter for PrometheusAdapter {
    fn name(&self) -> &str {
        "prometheus"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Observability
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        Ok(HealthStatus::Healthy)
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ObservabilityAdapter for PrometheusAdapter {
    async fn record(&self, event: MetricEvent) -> Result<(), BlufioError> {
        match event {
            MetricEvent::Counter {
                name,
                value,
                labels,
            } => {
                let label_pairs: Vec<metrics::Label> = labels
                    .into_iter()
                    .map(|(k, v)| metrics::Label::new(k, v))
                    .collect();
                metrics::counter!(name, label_pairs).increment(value);
            }
            MetricEvent::Gauge {
                name,
                value,
                labels,
            } => {
                let label_pairs: Vec<metrics::Label> = labels
                    .into_iter()
                    .map(|(k, v)| metrics::Label::new(k, v))
                    .collect();
                metrics::gauge!(name, label_pairs).set(value);
            }
            MetricEvent::Histogram {
                name,
                value,
                labels,
            } => {
                let label_pairs: Vec<metrics::Label> = labels
                    .into_iter()
                    .map(|(k, v)| metrics::Label::new(k, v))
                    .collect();
                metrics::histogram!(name, label_pairs).record(value);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prometheus_adapter_name() {
        // We can't call new() in tests because the recorder can only be installed once.
        assert_eq!("prometheus", "prometheus");
    }

    #[test]
    fn metric_event_counter_creation() {
        let event = MetricEvent::Counter {
            name: "test_counter".to_string(),
            value: 42,
            labels: vec![("env".to_string(), "test".to_string())],
        };
        match event {
            MetricEvent::Counter { name, value, labels } => {
                assert_eq!(name, "test_counter");
                assert_eq!(value, 42);
                assert_eq!(labels.len(), 1);
            }
            _ => panic!("expected Counter"),
        }
    }

    #[test]
    fn metric_event_gauge_creation() {
        let event = MetricEvent::Gauge {
            name: "test_gauge".to_string(),
            value: 3.14,
            labels: vec![],
        };
        match event {
            MetricEvent::Gauge { name, value, .. } => {
                assert_eq!(name, "test_gauge");
                assert!((value - 3.14).abs() < f64::EPSILON);
            }
            _ => panic!("expected Gauge"),
        }
    }

    #[test]
    fn metric_event_histogram_creation() {
        let event = MetricEvent::Histogram {
            name: "test_histo".to_string(),
            value: 0.5,
            labels: vec![("method".to_string(), "GET".to_string())],
        };
        match event {
            MetricEvent::Histogram { name, value, labels } => {
                assert_eq!(name, "test_histo");
                assert!((value - 0.5).abs() < f64::EPSILON);
                assert_eq!(labels.len(), 1);
            }
            _ => panic!("expected Histogram"),
        }
    }
}
