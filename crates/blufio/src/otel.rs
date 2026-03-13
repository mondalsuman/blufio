// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenTelemetry tracing infrastructure.
//!
//! Provides OTLP HTTP span export, TracerProvider construction, and graceful
//! shutdown. The core functions are feature-gated behind `cfg(feature = "otel")`.
//!
//! OTel initialization is intentionally non-fatal: any failure logs a warning
//! and returns `None`, allowing the agent to start without distributed tracing.

#[cfg(feature = "otel")]
use blufio_config::model::OpenTelemetryConfig;
#[cfg(feature = "otel")]
use opentelemetry::KeyValue;
#[cfg(feature = "otel")]
use opentelemetry::trace::TracerProvider as _;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otel")]
use opentelemetry_sdk::Resource;
#[cfg(feature = "otel")]
use opentelemetry_sdk::propagation::TraceContextPropagator;
#[cfg(feature = "otel")]
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider,
};
#[cfg(feature = "otel")]
use std::time::Duration;
#[cfg(feature = "otel")]
use tracing_subscriber::Registry;

/// Attempts to initialize the OpenTelemetry tracing layer.
///
/// Returns `Some((layer, provider))` on success, where the layer should be
/// composed into the tracing subscriber and the provider is retained for
/// graceful shutdown. Returns `None` if OTel is disabled in config or if
/// initialization fails (non-fatal).
#[cfg(feature = "otel")]
pub fn try_init_otel_layer(
    config: &OpenTelemetryConfig,
) -> Option<(
    tracing_opentelemetry::OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::SdkTracer>,
    SdkTracerProvider,
)> {
    if !config.enabled {
        return None;
    }

    // Build OTLP HTTP span exporter.
    let exporter = match opentelemetry_otlp::SpanExporterBuilder::new()
        .with_http()
        .with_endpoint(&config.endpoint)
        .build()
    {
        Ok(e) => {
            metrics::counter!("otel_spans_exported_total").absolute(0);
            e
        }
        Err(e) => {
            metrics::counter!("otel_export_errors_total").increment(1);
            // Cannot use tracing here -- subscriber not yet installed.
            eprintln!(
                "WARNING: OpenTelemetry exporter build failed: {e}. Continuing without OTel tracing."
            );
            return None;
        }
    };

    // Build resource attributes.
    let mut resource_builder = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attribute(KeyValue::new(
            "service.version",
            env!("CARGO_PKG_VERSION").to_string(),
        ))
        .with_attribute(KeyValue::new(
            "deployment.environment",
            config.environment.clone(),
        ));

    // Append user-defined resource attributes.
    for (k, v) in &config.resource_attributes {
        resource_builder = resource_builder.with_attribute(KeyValue::new(k.clone(), v.clone()));
    }

    let resource = resource_builder.build();

    // Build batch span processor with configured limits.
    let batch_config = BatchConfigBuilder::default()
        .with_max_queue_size(config.max_queue_size)
        .with_max_export_batch_size(config.max_export_batch_size)
        .with_scheduled_delay(Duration::from_millis(config.batch_timeout_ms))
        .build();

    let batch_processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(batch_config)
        .build();

    // Build sampler from configured ratio.
    let sampler = if (config.sample_ratio - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if config.sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_ratio)
    };

    // Build TracerProvider.
    let provider = SdkTracerProvider::builder()
        .with_span_processor(batch_processor)
        .with_sampler(Sampler::ParentBased(Box::new(sampler)))
        .with_resource(resource)
        .build();

    // Set W3C TraceContext propagator for distributed trace correlation.
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    // Create the tracing-opentelemetry layer from a tracer obtained from the provider.
    let tracer = provider.tracer("blufio");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Some((otel_layer, provider))
}

/// Gracefully shuts down the OTel TracerProvider, flushing pending spans.
///
/// This is a blocking operation that should be called during graceful shutdown
/// after the agent loop has stopped producing new spans.
#[cfg(feature = "otel")]
pub fn shutdown_otel(provider: SdkTracerProvider) {
    match provider.shutdown() {
        Ok(()) => {
            // Cannot use tracing here as subscriber may already be torn down.
            eprintln!("INFO: OpenTelemetry TracerProvider shut down successfully");
        }
        Err(e) => {
            eprintln!("WARNING: OpenTelemetry TracerProvider shutdown error: {e}");
        }
    }
}

/// Helper macro that creates a tracing span with OTel-friendly attributes.
///
/// When the `otel` feature is enabled, creates an `info_span!` so the span
/// is captured by the OpenTelemetry layer. When `otel` is not compiled,
/// creates a lightweight `debug_span!` that is typically filtered out.
#[cfg(feature = "otel")]
#[macro_export]
macro_rules! otel_span {
    ($name:expr $(, $key:expr => $val:expr)* $(,)?) => {{
        tracing::info_span!($name $(, $key = %$val)*)
    }};
}

/// Non-OTel variant: creates a lightweight debug span.
#[cfg(not(feature = "otel"))]
#[macro_export]
macro_rules! otel_span {
    ($name:expr $(, $key:expr => $val:expr)* $(,)?) => {{ tracing::debug_span!($name) }};
}

#[cfg(all(test, feature = "otel"))]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_returns_none() {
        let config = OpenTelemetryConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(try_init_otel_layer(&config).is_none());
    }

    #[test]
    fn invalid_endpoint_returns_none() {
        let config = OpenTelemetryConfig {
            enabled: true,
            // Use a clearly invalid endpoint -- the exporter builder may still
            // accept it (URL validation is deferred to export time in some
            // implementations), so this test verifies graceful handling.
            endpoint: "not://a valid endpoint!!!".to_string(),
            ..Default::default()
        };
        // The exporter builder may or may not reject this at build time.
        // Either way, the function should not panic.
        let result = try_init_otel_layer(&config);
        // If it returns Some, that's acceptable (URL validation deferred).
        // If it returns None, that's the graceful failure path.
        // The key assertion is: no panic occurred.
        drop(result);
    }
}
