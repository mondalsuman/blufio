// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prometheus metric registration for the injection defense subsystem.
//!
//! Uses the facade pattern (describe_counter!, counter!) following the
//! established convention in blufio-context and blufio-agent.

use metrics::{counter, describe_counter, describe_histogram, histogram};

/// Register all injection defense Prometheus metrics.
///
/// Call this once at startup (e.g., in `serve.rs` alongside other metric
/// registrations).
pub fn register_injection_metrics() {
    // L1: Input detection
    describe_counter!(
        "injection_input_detections_total",
        "Total injection pattern detections on input"
    );

    // L3: HMAC boundary validation
    describe_counter!(
        "hmac_validations_total",
        "Total HMAC boundary token validations"
    );

    // L4: Output screening
    describe_counter!(
        "injection_output_screenings_total",
        "Total output screening detections"
    );

    // L5: HITL confirmations
    describe_counter!(
        "hitl_confirmations_total",
        "Total HITL tool execution confirmations"
    );
    describe_counter!("hitl_denials_total", "Total HITL tool execution denials");
    describe_counter!("hitl_timeouts_total", "Total HITL confirmation timeouts");

    // Canary token detection
    describe_counter!(
        "injection_canary_detections_total",
        "Total canary token leak detections in LLM output"
    );

    // Scan duration histogram for p50/p95/p99 tracking
    describe_histogram!(
        "injection_scan_duration_seconds",
        "L1 injection scan pipeline duration"
    );
}

/// Record an L1 input detection event.
///
/// The `category` label enables per-category Prometheus metric breakdowns.
/// Callers should pass the matched injection category (e.g., `"role_hijacking"`).
pub fn record_input_detection(source_type: &str, action: &str, category: &str) {
    counter!("injection_input_detections_total", "source_type" => source_type.to_string(), "action" => action.to_string(), "category" => category.to_string())
        .increment(1);
}

/// Record an L3 HMAC validation result.
pub fn record_hmac_validation(zone: &str, result: &str) {
    counter!("hmac_validations_total", "zone" => zone.to_string(), "result" => result.to_string())
        .increment(1);
}

/// Record an L4 output screening detection.
pub fn record_output_screening(detection_type: &str, action: &str) {
    counter!("injection_output_screenings_total", "detection_type" => detection_type.to_string(), "action" => action.to_string())
        .increment(1);
}

/// Record an L5 HITL confirmation.
pub fn record_hitl_confirmation() {
    counter!("hitl_confirmations_total").increment(1);
}

/// Record an L5 HITL denial.
pub fn record_hitl_denial() {
    counter!("hitl_denials_total").increment(1);
}

/// Record an L5 HITL timeout.
pub fn record_hitl_timeout() {
    counter!("hitl_timeouts_total").increment(1);
}

/// Record L3 boundary validation successes.
pub fn record_boundary_validations(count: u64) {
    counter!("hmac_validations_total", "result" => "success").increment(count);
}

/// Record L3 boundary validation failures.
pub fn record_boundary_failures(count: u64) {
    counter!("hmac_validations_total", "result" => "failure").increment(count);
}

/// Record a canary token leak detection.
///
/// `token_type` is `"global"` or `"session"`.
pub fn record_canary_detection(token_type: &str) {
    counter!("injection_canary_detections_total", "token_type" => token_type.to_string())
        .increment(1);
}

/// Record L1 scan pipeline duration for histogram tracking.
pub fn record_scan_duration(duration_secs: f64) {
    histogram!("injection_scan_duration_seconds").record(duration_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_canary_detection_does_not_panic() {
        record_canary_detection("global");
        record_canary_detection("session");
    }

    #[test]
    fn record_scan_duration_does_not_panic() {
        record_scan_duration(0.015);
        record_scan_duration(0.0);
    }

    #[test]
    fn record_input_detection_with_category_does_not_panic() {
        record_input_detection("user", "logged", "role_hijacking");
        record_input_detection("mcp", "blocked", "data_exfiltration");
    }

    #[test]
    fn register_injection_metrics_does_not_panic() {
        register_injection_metrics();
    }
}
