// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prometheus metric registration for the injection defense subsystem.
//!
//! Uses the facade pattern (describe_counter!, counter!) following the
//! established convention in blufio-context and blufio-agent.

use metrics::{counter, describe_counter};

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
    describe_counter!(
        "hitl_denials_total",
        "Total HITL tool execution denials"
    );
    describe_counter!(
        "hitl_timeouts_total",
        "Total HITL confirmation timeouts"
    );
}

/// Record an L1 input detection event.
pub fn record_input_detection(source_type: &str, action: &str) {
    counter!("injection_input_detections_total", "source_type" => source_type.to_string(), "action" => action.to_string())
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
