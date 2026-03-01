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
    describe_gauge!("blufio_active_sessions", "Currently active sessions");
    describe_gauge!(
        "blufio_budget_remaining_usd",
        "Remaining daily budget in USD"
    );
    describe_histogram!(
        "blufio_response_latency_seconds",
        "LLM response latency in seconds"
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
