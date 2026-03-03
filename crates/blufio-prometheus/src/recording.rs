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

/// Record an error by type.
pub fn record_error(error_type: &str) {
    metrics::counter!("blufio_errors_total", "type" => error_type.to_string()).increment(1);
}

// ---- MCP metrics (INTG-04) ----

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
