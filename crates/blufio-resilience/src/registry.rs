// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Central registry holding one circuit breaker per external dependency.
//!
//! The registry is constructed once at startup from a config map and is
//! immutable at the map level (no runtime insertion/removal). Individual
//! breakers are protected by `std::sync::Mutex` for microsecond-hold-time
//! state updates.

use std::collections::HashMap;
use std::sync::Mutex;

use blufio_core::error::BlufioError;

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::clock::{Clock, RealClock};
use crate::snapshot::{CircuitBreakerSnapshot, CircuitBreakerTransition};

/// Central registry of circuit breakers for all external dependencies.
///
/// Thread-safe: the map is immutable after construction; individual breakers
/// are wrapped in `std::sync::Mutex`.
pub struct CircuitBreakerRegistry {
    breakers: HashMap<String, Mutex<CircuitBreaker>>,
}

impl CircuitBreakerRegistry {
    /// Create a new registry with one breaker per entry, using [`RealClock`].
    pub fn new(configs: HashMap<String, CircuitBreakerConfig>) -> Self {
        Self::new_with_clock_factory(configs, || Box::new(RealClock))
    }

    /// Create a new registry with a custom clock factory (for testing).
    pub fn new_with_clock_factory(
        configs: HashMap<String, CircuitBreakerConfig>,
        clock_factory: impl Fn() -> Box<dyn Clock + Send>,
    ) -> Self {
        let breakers = configs
            .into_iter()
            .map(|(name, config)| {
                let clock = clock_factory();
                let breaker = CircuitBreaker::new(name.clone(), config, clock);
                (name, Mutex::new(breaker))
            })
            .collect();
        Self { breakers }
    }

    /// Check whether a call to the named dependency should be allowed.
    ///
    /// Returns `Err(BlufioError)` if the dependency is unknown or the
    /// breaker is Open.
    pub fn check(&self, name: &str) -> Result<(), BlufioError> {
        let breaker = self
            .breakers
            .get(name)
            .ok_or_else(|| BlufioError::Internal(format!("unknown circuit breaker: {name}")))?;
        breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check()
    }

    /// Record the result of a call to the named dependency.
    ///
    /// Returns `Some(transition)` if the breaker changed state.
    /// Returns `None` if the dependency is unknown or no state change occurred.
    pub fn record_result(
        &self,
        name: &str,
        success: bool,
    ) -> Option<CircuitBreakerTransition> {
        let breaker = self.breakers.get(name)?;
        breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_result(success)
    }

    /// Signal that the current HalfOpen probe for the named dependency has completed.
    pub fn record_probe_complete(&self, name: &str) {
        if let Some(breaker) = self.breakers.get(name) {
            breaker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .record_probe_complete();
        }
    }

    /// Get a read-only snapshot of the named breaker.
    pub fn snapshot(&self, name: &str) -> Option<CircuitBreakerSnapshot> {
        let breaker = self.breakers.get(name)?;
        Some(
            breaker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .snapshot(),
        )
    }

    /// Get snapshots of all breakers.
    pub fn all_snapshots(&self) -> HashMap<String, CircuitBreakerSnapshot> {
        self.breakers
            .iter()
            .map(|(name, mutex)| {
                let snap = mutex
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .snapshot();
                (name.clone(), snap)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use crate::snapshot::CircuitBreakerState;
    use std::sync::Arc;
    use std::time::Duration;

    /// Wrapper to use Arc<MockClock> as Box<dyn Clock + Send>.
    struct ArcClock(Arc<MockClock>);
    impl Clock for ArcClock {
        fn now(&self) -> std::time::Instant {
            self.0.now()
        }
    }

    fn make_registry(
        deps: &[&str],
    ) -> (CircuitBreakerRegistry, Arc<MockClock>) {
        let clock = Arc::new(MockClock::new());
        let clock_ref = clock.clone();
        let configs: HashMap<String, CircuitBreakerConfig> = deps
            .iter()
            .map(|name| (name.to_string(), CircuitBreakerConfig::default()))
            .collect();
        let registry = CircuitBreakerRegistry::new_with_clock_factory(configs, move || {
            Box::new(ArcClock(clock_ref.clone()))
        });
        (registry, clock)
    }

    #[test]
    fn registry_creates_breakers_for_all_deps() {
        let (registry, _clock) = make_registry(&["anthropic", "openai", "telegram"]);
        let snapshots = registry.all_snapshots();
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots.contains_key("anthropic"));
        assert!(snapshots.contains_key("openai"));
        assert!(snapshots.contains_key("telegram"));
    }

    #[test]
    fn registry_check_delegates_to_breaker() {
        let (registry, _clock) = make_registry(&["anthropic"]);
        // Closed breaker should allow check
        assert!(registry.check("anthropic").is_ok());
    }

    #[test]
    fn registry_check_unknown_returns_error() {
        let (registry, _clock) = make_registry(&["anthropic"]);
        let err = registry.check("unknown").unwrap_err();
        assert!(err.to_string().contains("unknown circuit breaker"));
    }

    #[test]
    fn registry_record_result_delegates_and_returns_transition() {
        let (registry, _clock) = make_registry(&["anthropic"]);

        // 5 failures to trip the breaker
        for i in 0..4 {
            let t = registry.record_result("anthropic", false);
            assert!(t.is_none(), "unexpected transition at failure {}", i + 1);
        }
        let t = registry.record_result("anthropic", false);
        assert!(t.is_some());
        let transition = t.unwrap();
        assert_eq!(transition.dependency, "anthropic");
        assert_eq!(transition.from_state, CircuitBreakerState::Closed);
        assert_eq!(transition.to_state, CircuitBreakerState::Open);
    }

    #[test]
    fn registry_record_result_unknown_returns_none() {
        let (registry, _clock) = make_registry(&["anthropic"]);
        assert!(registry.record_result("unknown", true).is_none());
    }

    #[test]
    fn registry_snapshot_returns_individual_state() {
        let (registry, _clock) = make_registry(&["anthropic"]);
        let snap = registry.snapshot("anthropic").unwrap();
        assert_eq!(snap.name, "anthropic");
        assert_eq!(snap.state, CircuitBreakerState::Closed);
    }

    #[test]
    fn registry_snapshot_unknown_returns_none() {
        let (registry, _clock) = make_registry(&["anthropic"]);
        assert!(registry.snapshot("unknown").is_none());
    }

    #[test]
    fn registry_breakers_are_independent() {
        let (registry, _clock) = make_registry(&["anthropic", "openai"]);

        // Trip anthropic
        for _ in 0..5 {
            registry.record_result("anthropic", false);
        }

        // anthropic is Open, openai is still Closed
        assert_eq!(
            registry.snapshot("anthropic").unwrap().state,
            CircuitBreakerState::Open
        );
        assert_eq!(
            registry.snapshot("openai").unwrap().state,
            CircuitBreakerState::Closed
        );
    }

    #[test]
    fn registry_probe_complete_delegates() {
        let (registry, clock) = make_registry(&["anthropic"]);

        // Trip to Open
        for _ in 0..5 {
            registry.record_result("anthropic", false);
        }

        // Advance past timeout
        clock.advance(Duration::from_secs(61));
        assert!(registry.check("anthropic").is_ok()); // transitions to HalfOpen, sets probing

        // Complete probe
        registry.record_probe_complete("anthropic");

        // Can start another probe
        assert!(registry.check("anthropic").is_ok());
    }

    #[test]
    fn registry_all_snapshots_returns_all() {
        let deps = ["a", "b", "c", "d", "e"];
        let (registry, _clock) = make_registry(&deps);
        let snaps = registry.all_snapshots();
        assert_eq!(snaps.len(), 5);
        for dep in &deps {
            assert!(snaps.contains_key(*dep));
        }
    }
}
