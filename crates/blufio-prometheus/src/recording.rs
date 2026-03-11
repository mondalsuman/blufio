// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Metric registration and recording helpers.
//!
//! Uses the metrics-rs facade so any recorder (Prometheus, statsd, etc.)
//! can collect these metrics.

use metrics::{describe_counter, describe_gauge, describe_histogram};

/// Register all Blufio metric descriptions.
///
/// Called once at startup after the recorder is installed.
pub fn register_metrics() {
    describe_counter!("blufio_messages_total", "Total messages processed");
    describe_counter!("blufio_tokens_total", "Total tokens consumed");
    describe_counter!("blufio_errors_total", "Total errors by type");
    describe_gauge!("blufio_active_sessions", "Currently active sessions");
    describe_gauge!(
        "blufio_budget_remaining_usd",
        "Remaining daily budget in USD"
    );
    describe_gauge!("blufio_memory_heap_bytes", "jemalloc allocated heap bytes");
    describe_gauge!(
        "blufio_memory_rss_bytes",
        "Process RSS from /proc/self/statm"
    );
    describe_gauge!("blufio_memory_resident_bytes", "jemalloc resident bytes");
    describe_gauge!(
        "blufio_memory_pressure",
        "Memory pressure indicator (0=normal, 1=warning)"
    );
    describe_histogram!(
        "blufio_response_latency_seconds",
        "LLM response latency in seconds"
    );

    // MCP metrics (INTG-04)
    describe_counter!(
        "blufio_mcp_connections_total",
        "Total MCP connections by transport"
    );
    describe_gauge!(
        "blufio_mcp_active_connections",
        "Currently active MCP connections"
    );
    describe_histogram!(
        "blufio_mcp_tool_response_size_bytes",
        "MCP tool response sizes in bytes"
    );
    describe_gauge!(
        "blufio_mcp_context_utilization_ratio",
        "Context window utilization ratio"
    );

    register_resilience_metrics();
    register_classification_metrics();
    register_memory_validation_metrics();
}

/// Record a processed message.
pub fn record_message(channel: &str) {
    metrics::counter!("blufio_messages_total", "channel" => channel.to_string()).increment(1);
}

/// Record token consumption.
pub fn record_tokens(model: &str, input: u32, output: u32) {
    metrics::counter!("blufio_tokens_total", "model" => model.to_string(), "type" => "input")
        .increment(input as u64);
    metrics::counter!("blufio_tokens_total", "model" => model.to_string(), "type" => "output")
        .increment(output as u64);
}

/// Set the number of active sessions.
pub fn set_active_sessions(count: f64) {
    metrics::gauge!("blufio_active_sessions").set(count);
}

/// Set the remaining budget in USD.
pub fn set_budget_remaining(usd: f64) {
    metrics::gauge!("blufio_budget_remaining_usd").set(usd);
}

/// Record response latency.
pub fn record_latency(seconds: f64) {
    metrics::histogram!("blufio_response_latency_seconds").record(seconds);
}

/// Set jemalloc allocated heap bytes.
pub fn set_memory_heap(bytes: f64) {
    metrics::gauge!("blufio_memory_heap_bytes").set(bytes);
}

/// Set process RSS bytes (from /proc/self/statm on Linux).
pub fn set_memory_rss(bytes: f64) {
    metrics::gauge!("blufio_memory_rss_bytes").set(bytes);
}

/// Set jemalloc resident bytes.
pub fn set_memory_resident(bytes: f64) {
    metrics::gauge!("blufio_memory_resident_bytes").set(bytes);
}

/// Set memory pressure indicator (0.0 = normal, 1.0 = warning/shedding).
pub fn set_memory_pressure(pressure: f64) {
    metrics::gauge!("blufio_memory_pressure").set(pressure);
}

/// Record an error by type (legacy single-label API).
pub fn record_error(error_type: &str) {
    metrics::counter!("blufio_errors_total", "type" => error_type.to_string()).increment(1);
}

/// Record an error with structured classification labels.
///
/// Uses 3 labels: `category`, `failure_mode`, `severity`.
pub fn record_error_classified(category: &str, failure_mode: &str, severity: &str) {
    metrics::counter!(
        "blufio_errors_total",
        "category" => category.to_string(),
        "failure_mode" => failure_mode.to_string(),
        "severity" => severity.to_string(),
    )
    .increment(1);
}

/// Convenience: record a classified error directly from a [`BlufioError`].
///
/// Extracts `category()`, `failure_mode()`, and `severity()` from the error
/// and calls [`record_error_classified`].
pub fn record_classified_error(error: &blufio_core::BlufioError) {
    record_error_classified(
        &error.category().to_string(),
        &error.failure_mode().to_string(),
        &error.severity().to_string(),
    );
}

// ---- MCP metrics (INTG-04) ----
// Call sites:
//   record_mcp_connection()         -> manager.rs connect_all() on successful connect
//   set_mcp_active_connections()    -> serve.rs after connect_all() returns
//   record_mcp_tool_response_size() -> external_tool.rs invoke() after response
//   set_mcp_context_utilization()   -> not yet wired (requires context engine integration)

/// Record an MCP connection by transport type.
pub fn record_mcp_connection(transport: &str) {
    metrics::counter!("blufio_mcp_connections_total", "transport" => transport.to_string())
        .increment(1);
}

/// Set the number of currently active MCP connections.
pub fn set_mcp_active_connections(count: f64) {
    metrics::gauge!("blufio_mcp_active_connections").set(count);
}

/// Record an MCP tool response size in bytes.
pub fn record_mcp_tool_response_size(bytes: f64) {
    metrics::histogram!("blufio_mcp_tool_response_size_bytes").record(bytes);
}

/// Set the context window utilization ratio (0.0 to 1.0).
pub fn set_mcp_context_utilization(ratio: f64) {
    metrics::gauge!("blufio_mcp_context_utilization_ratio").set(ratio);
}

// ---- Resilience metrics (CB-04, CB-05) ----

/// Register classification metric descriptions.
///
/// Called from [`register_metrics()`] at startup.
fn register_classification_metrics() {
    describe_counter!(
        "blufio_classification_blocked_total",
        "Total classification enforcement actions by level and action"
    );
}

/// Record a classification enforcement action.
///
/// Tracks when content is blocked from export, context inclusion, or requires
/// log redaction due to its classification level.
///
/// `action` is one of: "export", "context_include", "log_redact"
pub fn record_classification_blocked(level: &str, action: &str) {
    metrics::counter!(
        "blufio_classification_blocked_total",
        "level" => level.to_string(),
        "action" => action.to_string(),
    )
    .increment(1);
}

/// Register resilience metric descriptions.
///
/// Called from [`register_metrics()`] at startup.
fn register_resilience_metrics() {
    describe_gauge!(
        "blufio_circuit_breaker_state",
        "Circuit breaker state per dependency (0=closed, 1=half_open, 2=open)"
    );
    describe_gauge!(
        "blufio_degradation_level",
        "Current system degradation level (0=L0 FullyOperational through 5=L5 SafeShutdown)"
    );
    describe_counter!(
        "blufio_circuit_breaker_transitions_total",
        "Total circuit breaker state transitions by dependency and direction"
    );
}

/// Record the current state of a circuit breaker for a dependency.
///
/// `state` is the numeric value: 0=closed, 1=half_open, 2=open.
pub fn record_circuit_breaker_state(dependency: &str, state: u8) {
    metrics::gauge!("blufio_circuit_breaker_state", "dependency" => dependency.to_string())
        .set(state as f64);
}

/// Record the current system degradation level (0-5).
pub fn record_degradation_level(level: u8) {
    metrics::gauge!("blufio_degradation_level").set(level as f64);
}

/// Record a circuit breaker state transition.
pub fn record_circuit_breaker_transition(dependency: &str, from: &str, to: &str) {
    metrics::counter!(
        "blufio_circuit_breaker_transitions_total",
        "dependency" => dependency.to_string(),
        "from" => from.to_string(),
        "to" => to.to_string(),
    )
    .increment(1);
}

// ---- Memory validation metrics (MEME-06) ----

/// Register memory validation metric descriptions.
///
/// Called from [`register_metrics()`] at startup.
fn register_memory_validation_metrics() {
    describe_counter!(
        "blufio_memory_validation_duplicates_total",
        "Total duplicate memories detected by validation"
    );
    describe_counter!(
        "blufio_memory_validation_stale_total",
        "Total stale memories detected by validation"
    );
    describe_counter!(
        "blufio_memory_validation_conflicts_total",
        "Total conflicting memories detected by validation"
    );
    describe_gauge!(
        "blufio_memory_active_count",
        "Current count of active memories"
    );
}

/// Record duplicate memories found during validation.
pub fn record_validation_duplicates(count: u64) {
    metrics::counter!("blufio_memory_validation_duplicates_total").increment(count);
}

/// Record stale memories found during validation.
pub fn record_validation_stale(count: u64) {
    metrics::counter!("blufio_memory_validation_stale_total").increment(count);
}

/// Record conflicting memories found during validation.
pub fn record_validation_conflicts(count: u64) {
    metrics::counter!("blufio_memory_validation_conflicts_total").increment(count);
}

/// Set the current count of active memories.
pub fn set_memory_active_count(count: f64) {
    metrics::gauge!("blufio_memory_active_count").set(count);
}
