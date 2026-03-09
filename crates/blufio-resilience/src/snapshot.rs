// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only snapshot and transition types for circuit breaker state inspection.

use std::fmt;
use std::time::Instant;

/// The three states of the circuit breaker finite state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CircuitBreakerState {
    /// Normal operation -- all calls pass through.
    Closed,
    /// Breaker tripped -- calls are fast-failed immediately.
    Open,
    /// Recovery probing -- a limited number of calls are allowed through.
    HalfOpen,
}

impl CircuitBreakerState {
    /// Returns the string representation: `"closed"`, `"open"`, or `"half_open"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::HalfOpen => "half_open",
        }
    }

    /// Returns the numeric value for Prometheus gauges:
    /// 0 = Closed, 1 = HalfOpen, 2 = Open.
    pub fn as_numeric(&self) -> u8 {
        match self {
            Self::Closed => 0,
            Self::HalfOpen => 1,
            Self::Open => 2,
        }
    }
}

impl fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A read-only snapshot of a circuit breaker's current state.
#[derive(Debug, Clone)]
pub struct CircuitBreakerSnapshot {
    /// Name of the dependency this breaker protects.
    pub name: String,
    /// Current FSM state.
    pub state: CircuitBreakerState,
    /// Number of consecutive failures recorded in Closed state.
    pub failure_count: u32,
    /// Number of consecutive successes recorded in HalfOpen state.
    pub consecutive_successes: u32,
    /// When the last failure was recorded (if any).
    pub last_failure: Option<Instant>,
    /// When the last success was recorded (if any).
    pub last_success: Option<Instant>,
    /// When the state last changed.
    pub last_state_change: Instant,
}

/// Describes a state transition that occurred in a circuit breaker.
///
/// Returned by `record_result()` when a state change happens.
/// The caller publishes this to the EventBus after releasing the breaker lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitBreakerTransition {
    /// Name of the dependency whose breaker transitioned.
    pub dependency: String,
    /// Previous state.
    pub from_state: CircuitBreakerState,
    /// New state.
    pub to_state: CircuitBreakerState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_as_str() {
        assert_eq!(CircuitBreakerState::Closed.as_str(), "closed");
        assert_eq!(CircuitBreakerState::Open.as_str(), "open");
        assert_eq!(CircuitBreakerState::HalfOpen.as_str(), "half_open");
    }

    #[test]
    fn state_display() {
        assert_eq!(format!("{}", CircuitBreakerState::Closed), "closed");
        assert_eq!(format!("{}", CircuitBreakerState::Open), "open");
        assert_eq!(format!("{}", CircuitBreakerState::HalfOpen), "half_open");
    }

    #[test]
    fn state_numeric_for_prometheus() {
        assert_eq!(CircuitBreakerState::Closed.as_numeric(), 0);
        assert_eq!(CircuitBreakerState::HalfOpen.as_numeric(), 1);
        assert_eq!(CircuitBreakerState::Open.as_numeric(), 2);
    }

    #[test]
    fn snapshot_clone() {
        let snap = CircuitBreakerSnapshot {
            name: "anthropic".into(),
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            consecutive_successes: 0,
            last_failure: None,
            last_success: None,
            last_state_change: Instant::now(),
        };
        let cloned = snap.clone();
        assert_eq!(cloned.name, "anthropic");
        assert_eq!(cloned.state, CircuitBreakerState::Closed);
    }

    #[test]
    fn transition_equality() {
        let t1 = CircuitBreakerTransition {
            dependency: "openai".into(),
            from_state: CircuitBreakerState::Closed,
            to_state: CircuitBreakerState::Open,
        };
        let t2 = t1.clone();
        assert_eq!(t1, t2);
    }
}
