// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core circuit breaker finite state machine.
//!
//! Implements a 3-state FSM (Closed -> Open -> HalfOpen -> Closed) with
//! configurable failure thresholds, reset timeouts, and half-open probe counts.
//! Uses an injectable [`Clock`] trait for deterministic testing.

use std::time::{Duration, Instant};

use blufio_core::error::BlufioError;

use crate::clock::Clock;
use crate::snapshot::{CircuitBreakerSnapshot, CircuitBreakerState, CircuitBreakerTransition};

/// Configuration for a single circuit breaker instance.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before transitioning Closed -> Open.
    pub failure_threshold: u32,
    /// How long the breaker stays Open before transitioning to HalfOpen.
    pub reset_timeout: Duration,
    /// Number of consecutive successful probes in HalfOpen before closing.
    pub half_open_probes: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(60),
            half_open_probes: 3,
        }
    }
}

/// A circuit breaker protecting a single external dependency.
///
/// Thread safety: individual instances are NOT thread-safe. The
/// [`CircuitBreakerRegistry`](crate::registry::CircuitBreakerRegistry) wraps
/// each breaker in a `Mutex`.
pub struct CircuitBreaker {
    name: String,
    state: CircuitBreakerState,
    config: CircuitBreakerConfig,
    failure_count: u32,
    consecutive_successes: u32,
    last_failure: Option<Instant>,
    last_success: Option<Instant>,
    last_state_change: Instant,
    probing: bool,
    clock: Box<dyn Clock + Send>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker for the named dependency.
    pub fn new(
        name: impl Into<String>,
        config: CircuitBreakerConfig,
        clock: Box<dyn Clock + Send>,
    ) -> Self {
        let now = clock.now();
        Self {
            name: name.into(),
            state: CircuitBreakerState::Closed,
            config,
            failure_count: 0,
            consecutive_successes: 0,
            last_failure: None,
            last_success: None,
            last_state_change: now,
            probing: false,
            clock,
        }
    }

    /// Check whether a call should be allowed through.
    ///
    /// - **Closed:** Always returns `Ok(())`.
    /// - **Open:** If `reset_timeout` has elapsed, transitions to HalfOpen and
    ///   allows the call as a probe. Otherwise returns `Err(CircuitOpen)`.
    /// - **HalfOpen:** If no probe is in-flight, marks as probing and returns
    ///   `Ok(())`. Otherwise returns `Err(CircuitOpen)` (serialized probing).
    pub fn check(&mut self) -> Result<(), BlufioError> {
        match self.state {
            CircuitBreakerState::Closed => Ok(()),
            CircuitBreakerState::Open => {
                let now = self.clock.now();
                let elapsed = now.duration_since(self.last_state_change);
                if elapsed >= self.config.reset_timeout {
                    // Lazy transition to HalfOpen
                    self.state = CircuitBreakerState::HalfOpen;
                    self.last_state_change = now;
                    self.consecutive_successes = 0;
                    self.probing = true;
                    Ok(())
                } else {
                    Err(BlufioError::circuit_open(&self.name))
                }
            }
            CircuitBreakerState::HalfOpen => {
                if !self.probing {
                    self.probing = true;
                    Ok(())
                } else {
                    Err(BlufioError::circuit_open(&self.name))
                }
            }
        }
    }

    /// Signal that the current HalfOpen probe call has completed.
    ///
    /// Must be called after every probe attempt (success or failure) and
    /// *before* `record_result()`, to release the probe slot.
    pub fn record_probe_complete(&mut self) {
        self.probing = false;
    }

    /// Record the outcome of a call to the protected dependency.
    ///
    /// Returns `Some(transition)` if the state changed, `None` otherwise.
    pub fn record_result(&mut self, success: bool) -> Option<CircuitBreakerTransition> {
        let now = self.clock.now();

        match self.state {
            CircuitBreakerState::Closed => {
                if success {
                    self.failure_count = 0;
                    self.last_success = Some(now);
                    None
                } else {
                    self.failure_count += 1;
                    self.last_failure = Some(now);
                    if self.failure_count >= self.config.failure_threshold {
                        let from = self.state;
                        self.state = CircuitBreakerState::Open;
                        self.last_state_change = now;
                        Some(CircuitBreakerTransition {
                            dependency: self.name.clone(),
                            from_state: from,
                            to_state: CircuitBreakerState::Open,
                        })
                    } else {
                        None
                    }
                }
            }
            CircuitBreakerState::Open => {
                // Calls in Open state shouldn't happen (check() blocks them),
                // but if they do, ignore the result.
                None
            }
            CircuitBreakerState::HalfOpen => {
                if success {
                    self.consecutive_successes += 1;
                    self.last_success = Some(now);
                    if self.consecutive_successes >= self.config.half_open_probes {
                        let from = self.state;
                        self.state = CircuitBreakerState::Closed;
                        self.failure_count = 0;
                        self.last_state_change = now;
                        Some(CircuitBreakerTransition {
                            dependency: self.name.clone(),
                            from_state: from,
                            to_state: CircuitBreakerState::Closed,
                        })
                    } else {
                        None
                    }
                } else {
                    self.last_failure = Some(now);
                    self.consecutive_successes = 0;
                    let from = self.state;
                    self.state = CircuitBreakerState::Open;
                    self.last_state_change = now;
                    Some(CircuitBreakerTransition {
                        dependency: self.name.clone(),
                        from_state: from,
                        to_state: CircuitBreakerState::Open,
                    })
                }
            }
        }
    }

    /// Returns a read-only snapshot of the current breaker state.
    pub fn snapshot(&self) -> CircuitBreakerSnapshot {
        CircuitBreakerSnapshot {
            name: self.name.clone(),
            state: self.state,
            failure_count: self.failure_count,
            consecutive_successes: self.consecutive_successes,
            last_failure: self.last_failure,
            last_success: self.last_success,
            last_state_change: self.last_state_change,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use std::sync::Arc;

    fn make_breaker(threshold: u32, timeout_secs: u64, probes: u32) -> (CircuitBreaker, Arc<MockClock>) {
        let clock = Arc::new(MockClock::new());
        let config = CircuitBreakerConfig {
            failure_threshold: threshold,
            reset_timeout: Duration::from_secs(timeout_secs),
            half_open_probes: probes,
        };
        // Clone Arc for the breaker (it needs Box<dyn Clock + Send>)
        let clock_box: Box<dyn Clock + Send> = Box::new(ArcClock(clock.clone()));
        let breaker = CircuitBreaker::new("test-dep", config, clock_box);
        (breaker, clock)
    }

    /// Wrapper to use Arc<MockClock> as Box<dyn Clock + Send>.
    struct ArcClock(Arc<MockClock>);
    impl Clock for ArcClock {
        fn now(&self) -> Instant {
            self.0.now()
        }
    }

    // --- Behavior 1: Closed state check() returns Ok ---
    #[test]
    fn closed_state_check_returns_ok() {
        let (mut breaker, _clock) = make_breaker(5, 60, 3);
        assert!(breaker.check().is_ok());
    }

    // --- Behavior 2: Consecutive failures trip to Open ---
    #[test]
    fn closed_transitions_to_open_after_threshold_failures() {
        let (mut breaker, _clock) = make_breaker(5, 60, 3);

        // First 4 failures should not trip
        for i in 0..4 {
            let transition = breaker.record_result(false);
            assert!(transition.is_none(), "unexpected transition at failure {}", i + 1);
        }
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Closed);

        // 5th failure trips to Open
        let transition = breaker.record_result(false);
        assert!(transition.is_some());
        let t = transition.unwrap();
        assert_eq!(t.from_state, CircuitBreakerState::Closed);
        assert_eq!(t.to_state, CircuitBreakerState::Open);
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Open);
    }

    // --- Behavior 3: Success resets failure count in Closed ---
    #[test]
    fn closed_success_resets_failure_count() {
        let (mut breaker, _clock) = make_breaker(5, 60, 3);

        // 4 failures
        for _ in 0..4 {
            breaker.record_result(false);
        }
        assert_eq!(breaker.snapshot().failure_count, 4);

        // 1 success resets to 0
        breaker.record_result(true);
        assert_eq!(breaker.snapshot().failure_count, 0);

        // Need 5 more failures to trip
        for _ in 0..4 {
            assert!(breaker.record_result(false).is_none());
        }
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Closed);

        // 5th new failure trips
        assert!(breaker.record_result(false).is_some());
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Open);
    }

    // --- Behavior 4: Open state fast-fails before timeout ---
    #[test]
    fn open_state_fast_fails_before_timeout() {
        let (mut breaker, _clock) = make_breaker(5, 60, 3);

        // Trip to Open
        for _ in 0..5 {
            breaker.record_result(false);
        }
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Open);

        // check() should fail
        let err = breaker.check().unwrap_err();
        assert!(err.to_string().contains("circuit breaker open"));
    }

    // --- Behavior 5: Open transitions to HalfOpen after timeout (lazy) ---
    #[test]
    fn open_transitions_to_half_open_after_timeout() {
        let (mut breaker, clock) = make_breaker(5, 60, 3);

        // Trip to Open
        for _ in 0..5 {
            breaker.record_result(false);
        }

        // Advance past reset_timeout
        clock.advance(Duration::from_secs(61));

        // check() transitions to HalfOpen and allows call
        assert!(breaker.check().is_ok());
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::HalfOpen);
    }

    // --- Behavior 6: HalfOpen allows 1 probe, serialized ---
    #[test]
    fn half_open_serialized_probing() {
        let (mut breaker, clock) = make_breaker(5, 60, 3);

        // Trip to Open then advance to HalfOpen
        for _ in 0..5 {
            breaker.record_result(false);
        }
        clock.advance(Duration::from_secs(61));
        assert!(breaker.check().is_ok()); // first probe allowed

        // Second call should fast-fail (probe in-flight)
        let err = breaker.check().unwrap_err();
        assert!(err.to_string().contains("circuit breaker open"));

        // Complete probe and record success
        breaker.record_probe_complete();
        breaker.record_result(true);

        // Now another probe should be allowed
        assert!(breaker.check().is_ok());
    }

    // --- Behavior 7: HalfOpen transitions to Closed after N probe successes ---
    #[test]
    fn half_open_closes_after_successful_probes() {
        let (mut breaker, clock) = make_breaker(5, 60, 3);

        // Trip to Open then advance to HalfOpen
        for _ in 0..5 {
            breaker.record_result(false);
        }
        clock.advance(Duration::from_secs(61));

        // 3 successful probes
        for i in 0..3 {
            assert!(breaker.check().is_ok(), "probe {} should be allowed", i + 1);
            breaker.record_probe_complete();
            let transition = breaker.record_result(true);
            if i < 2 {
                assert!(transition.is_none(), "no transition after probe {}", i + 1);
            } else {
                // 3rd success closes the breaker
                let t = transition.unwrap();
                assert_eq!(t.from_state, CircuitBreakerState::HalfOpen);
                assert_eq!(t.to_state, CircuitBreakerState::Closed);
            }
        }
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Closed);
    }

    // --- Behavior 8: HalfOpen transitions to Open on probe failure ---
    #[test]
    fn half_open_reopens_on_probe_failure() {
        let (mut breaker, clock) = make_breaker(5, 60, 3);

        // Trip to Open then advance to HalfOpen
        for _ in 0..5 {
            breaker.record_result(false);
        }
        clock.advance(Duration::from_secs(61));

        // Allow probe
        assert!(breaker.check().is_ok());
        breaker.record_probe_complete();

        // Probe fails -- back to Open
        let transition = breaker.record_result(false);
        assert!(transition.is_some());
        let t = transition.unwrap();
        assert_eq!(t.from_state, CircuitBreakerState::HalfOpen);
        assert_eq!(t.to_state, CircuitBreakerState::Open);
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Open);
    }

    // --- Behavior 9: record_result returns None when no state change ---
    #[test]
    fn record_result_returns_none_without_state_change() {
        let (mut breaker, _clock) = make_breaker(5, 60, 3);

        // Success in Closed -- no transition
        assert!(breaker.record_result(true).is_none());

        // Single failure in Closed -- no transition
        assert!(breaker.record_result(false).is_none());
    }

    // --- Behavior 10: snapshot() returns accurate data ---
    #[test]
    fn snapshot_returns_accurate_data() {
        let (mut breaker, _clock) = make_breaker(5, 60, 3);

        let snap = breaker.snapshot();
        assert_eq!(snap.name, "test-dep");
        assert_eq!(snap.state, CircuitBreakerState::Closed);
        assert_eq!(snap.failure_count, 0);
        assert_eq!(snap.consecutive_successes, 0);
        assert!(snap.last_failure.is_none());
        assert!(snap.last_success.is_none());

        // Record a failure
        breaker.record_result(false);
        let snap = breaker.snapshot();
        assert_eq!(snap.failure_count, 1);
        assert!(snap.last_failure.is_some());
    }

    // --- Behavior 11: MockClock enables deterministic timeout testing ---
    #[test]
    fn mock_clock_enables_timeout_testing() {
        let (mut breaker, clock) = make_breaker(5, 120, 3);

        // Trip to Open
        for _ in 0..5 {
            breaker.record_result(false);
        }

        // 60 seconds is not enough for 120s timeout
        clock.advance(Duration::from_secs(60));
        assert!(breaker.check().is_err());

        // 121 seconds total is enough
        clock.advance(Duration::from_secs(61));
        assert!(breaker.check().is_ok());
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::HalfOpen);
    }

    // --- Behavior: configurable threshold ---
    #[test]
    fn custom_failure_threshold() {
        let (mut breaker, _clock) = make_breaker(2, 60, 3);

        // 1 failure is not enough
        assert!(breaker.record_result(false).is_none());

        // 2nd failure trips
        let transition = breaker.record_result(false);
        assert!(transition.is_some());
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Open);
    }

    // --- Behavior: configurable half_open_probes ---
    #[test]
    fn custom_half_open_probes() {
        let (mut breaker, clock) = make_breaker(5, 60, 1);

        // Trip to Open
        for _ in 0..5 {
            breaker.record_result(false);
        }
        clock.advance(Duration::from_secs(61));

        // Only 1 probe needed to close
        assert!(breaker.check().is_ok());
        breaker.record_probe_complete();
        let transition = breaker.record_result(true);
        assert!(transition.is_some());
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Closed);
    }

    // --- Behavior: default config values ---
    #[test]
    fn default_config() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.reset_timeout, Duration::from_secs(60));
        assert_eq!(config.half_open_probes, 3);
    }

    // --- Behavior: CircuitOpen error properties ---
    #[test]
    fn circuit_open_error_not_retryable_not_tripping() {
        let err = BlufioError::circuit_open("test");
        assert!(!err.is_retryable());
        assert!(!err.trips_circuit_breaker());
    }

    // --- Behavior: full lifecycle Closed -> Open -> HalfOpen -> Closed ---
    #[test]
    fn full_lifecycle() {
        let (mut breaker, clock) = make_breaker(3, 30, 2);

        // Phase 1: Closed - 3 failures to Open
        for _ in 0..3 {
            breaker.record_result(false);
        }
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::Open);

        // Phase 2: Open - fast-fail
        assert!(breaker.check().is_err());

        // Phase 3: After timeout - HalfOpen
        clock.advance(Duration::from_secs(31));
        assert!(breaker.check().is_ok());
        assert_eq!(breaker.snapshot().state, CircuitBreakerState::HalfOpen);

        // Phase 4: 2 successful probes - Closed
        breaker.record_probe_complete();
        breaker.record_result(true);
        assert!(breaker.check().is_ok());
        breaker.record_probe_complete();
        let t = breaker.record_result(true).unwrap();
        assert_eq!(t.to_state, CircuitBreakerState::Closed);

        // Phase 5: Back to normal
        assert!(breaker.check().is_ok());
        assert_eq!(breaker.snapshot().failure_count, 0);
    }
}
