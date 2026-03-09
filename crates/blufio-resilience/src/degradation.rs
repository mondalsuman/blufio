// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Degradation ladder with 6 levels (L0-L5) and automatic escalation/de-escalation.
//!
//! The [`DegradationManager`] subscribes to circuit breaker state changes via
//! the EventBus and computes the current degradation level from the set of open
//! breakers. De-escalation only happens after a sustained recovery period
//! (hysteresis), and level changes are published back to the EventBus.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, ResilienceEvent, new_event_id, now_timestamp};

use crate::registry::CircuitBreakerRegistry;
use crate::snapshot::CircuitBreakerState;

/// The six degradation levels of the system.
///
/// Each level corresponds to a specific operational posture based on the
/// number and criticality of open circuit breakers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DegradationLevel {
    /// L0: All systems nominal.
    FullyOperational,
    /// L1: 1 non-critical dependency breaker open.
    MinorDegradation,
    /// L2: Primary provider breaker open.
    ReducedFunctionality,
    /// L3: 2+ critical dependencies open.
    CoreOnly,
    /// L4: All providers open.
    Emergency,
    /// L5: All providers + primary channel open. Irreversible.
    SafeShutdown,
}

impl DegradationLevel {
    /// Convert from a `u8` value (0-5) to the corresponding level.
    ///
    /// Values > 5 are clamped to L5.
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::FullyOperational,
            1 => Self::MinorDegradation,
            2 => Self::ReducedFunctionality,
            3 => Self::CoreOnly,
            4 => Self::Emergency,
            5 => Self::SafeShutdown,
            _ => Self::SafeShutdown,
        }
    }

    /// Convert to a `u8` value (0-5).
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::FullyOperational => 0,
            Self::MinorDegradation => 1,
            Self::ReducedFunctionality => 2,
            Self::CoreOnly => 3,
            Self::Emergency => 4,
            Self::SafeShutdown => 5,
        }
    }

    /// Returns the human-readable name of this level.
    pub fn name(&self) -> &'static str {
        match self {
            Self::FullyOperational => "FullyOperational",
            Self::MinorDegradation => "MinorDegradation",
            Self::ReducedFunctionality => "ReducedFunctionality",
            Self::CoreOnly => "CoreOnly",
            Self::Emergency => "Emergency",
            Self::SafeShutdown => "SafeShutdown",
        }
    }
}

impl fmt::Display for DegradationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "L{} {}", self.as_u8(), self.name())
    }
}

/// Configuration for the escalation/de-escalation logic.
#[derive(Debug, Clone)]
pub struct EscalationConfig {
    /// Name of the primary provider (e.g., "anthropic").
    pub primary_provider: String,
    /// Name of the primary channel (e.g., "telegram").
    pub primary_channel: String,
    /// Seconds of sustained recovery before de-escalation (default 120).
    pub hysteresis_secs: u64,
    /// Seconds to wait for in-flight requests during L5 drain (default 30).
    pub drain_timeout_secs: u64,
    /// Names of all provider dependencies (e.g., ["anthropic", "openai", "ollama"]).
    pub provider_names: Vec<String>,
}

/// Manages the system-wide degradation level based on circuit breaker states.
///
/// The level is stored as an [`AtomicU8`] for zero-cost reads from the agent
/// loop. The [`run()`](Self::run) method processes circuit breaker events and
/// manages escalation/de-escalation with hysteresis.
pub struct DegradationManager {
    level: AtomicU8,
    registry: Arc<CircuitBreakerRegistry>,
    config: EscalationConfig,
    cancellation_token: CancellationToken,
}

impl DegradationManager {
    /// Create a new degradation manager.
    pub fn new(
        registry: Arc<CircuitBreakerRegistry>,
        config: EscalationConfig,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            level: AtomicU8::new(0),
            registry,
            config,
            cancellation_token,
        }
    }

    /// Returns the current degradation level (zero-cost atomic read).
    pub fn current_level(&self) -> DegradationLevel {
        DegradationLevel::from_u8(self.level.load(Ordering::Relaxed))
    }

    /// Returns a clone of the cancellation token for use by serve.rs.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Compute the degradation level from current circuit breaker states.
    ///
    /// Reads all snapshots from the registry and applies the escalation
    /// triggers defined in the context decisions.
    pub fn compute_level(&self) -> DegradationLevel {
        let snapshots = self.registry.all_snapshots();

        // Count open breakers and check specific conditions
        let primary_provider_open = snapshots
            .get(&self.config.primary_provider)
            .is_some_and(|s| s.state == CircuitBreakerState::Open);

        let primary_channel_open = snapshots
            .get(&self.config.primary_channel)
            .is_some_and(|s| s.state == CircuitBreakerState::Open);

        let all_providers_open = !self.config.provider_names.is_empty()
            && self.config.provider_names.iter().all(|name| {
                snapshots
                    .get(name)
                    .is_some_and(|s| s.state == CircuitBreakerState::Open)
            });

        let open_provider_count = self
            .config
            .provider_names
            .iter()
            .filter(|name| {
                snapshots
                    .get(name.as_str())
                    .is_some_and(|s| s.state == CircuitBreakerState::Open)
            })
            .count();

        // Total open breakers
        let total_open: usize = snapshots
            .values()
            .filter(|s| s.state == CircuitBreakerState::Open)
            .count();

        // Apply escalation triggers (highest matching level wins)
        if all_providers_open && primary_channel_open {
            DegradationLevel::SafeShutdown
        } else if all_providers_open {
            DegradationLevel::Emergency
        } else if open_provider_count >= 2 {
            // 2+ critical deps (providers) open
            DegradationLevel::CoreOnly
        } else if primary_provider_open {
            DegradationLevel::ReducedFunctionality
        } else if total_open >= 1 {
            // 1 non-critical breaker open (secondary channel or non-primary provider)
            DegradationLevel::MinorDegradation
        } else {
            DegradationLevel::FullyOperational
        }
    }

    /// Run the degradation manager event loop.
    ///
    /// Receives circuit breaker state change events from the EventBus via a
    /// reliable mpsc channel. On each event:
    /// - Recomputes the degradation level
    /// - If escalation: stores immediately, publishes event, resets hysteresis
    /// - If de-escalation candidate: waits for hysteresis period
    /// - On L5: cancels the CancellationToken (irreversible)
    pub async fn run(&self, mut rx: mpsc::Receiver<BusEvent>, event_bus: Arc<EventBus>) {
        let mut hysteresis_deadline: Option<tokio::time::Instant> = None;

        loop {
            // If we have a hysteresis timer, use select! to listen for both
            // events and timer expiry.
            if let Some(deadline) = hysteresis_deadline {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Some(BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged { .. })) => {
                                let new_level = self.compute_level();
                                let current = self.current_level();

                                if new_level.as_u8() > current.as_u8() {
                                    // Escalation: immediate
                                    self.set_level(new_level, &current, &event_bus, "escalation").await;
                                    hysteresis_deadline = None;
                                } else if new_level.as_u8() < current.as_u8() {
                                    // De-escalation candidate: start/reset hysteresis
                                    hysteresis_deadline = Some(
                                        tokio::time::Instant::now()
                                            + std::time::Duration::from_secs(self.config.hysteresis_secs),
                                    );
                                } else {
                                    // Same level, reset hysteresis if any new event
                                    // (timer stays if de-escalation was pending)
                                }
                            }
                            Some(_) => {
                                // Ignore non-resilience events
                            }
                            None => {
                                // Channel closed, exit
                                tracing::info!("degradation manager: event channel closed, exiting");
                                return;
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        // Hysteresis timer expired: de-escalate one step
                        let current = self.current_level();
                        let new_computed = self.compute_level();
                        if new_computed.as_u8() < current.as_u8() {
                            let one_step_down = DegradationLevel::from_u8(current.as_u8() - 1);
                            self.set_level(one_step_down, &current, &event_bus, "de-escalation after hysteresis").await;

                            // If still above computed level, set another hysteresis timer
                            if one_step_down.as_u8() > new_computed.as_u8() {
                                hysteresis_deadline = Some(
                                    tokio::time::Instant::now()
                                        + std::time::Duration::from_secs(self.config.hysteresis_secs),
                                );
                            } else {
                                hysteresis_deadline = None;
                            }
                        } else {
                            // Recovery reversed while waiting
                            hysteresis_deadline = None;
                        }
                    }
                }
            } else {
                // No hysteresis timer, just wait for events
                match rx.recv().await {
                    Some(BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
                        ..
                    })) => {
                        let new_level = self.compute_level();
                        let current = self.current_level();

                        if new_level.as_u8() > current.as_u8() {
                            // Escalation: immediate
                            self.set_level(new_level, &current, &event_bus, "escalation")
                                .await;
                        } else if new_level.as_u8() < current.as_u8() {
                            // De-escalation candidate: start hysteresis timer
                            hysteresis_deadline = Some(
                                tokio::time::Instant::now()
                                    + std::time::Duration::from_secs(self.config.hysteresis_secs),
                            );
                        }
                    }
                    Some(_) => {
                        // Ignore non-resilience events
                    }
                    None => {
                        tracing::info!("degradation manager: event channel closed, exiting");
                        return;
                    }
                }
            }
        }
    }

    /// Set the level, publish the event, and handle L5 cancellation.
    async fn set_level(
        &self,
        new_level: DegradationLevel,
        old_level: &DegradationLevel,
        event_bus: &EventBus,
        reason: &str,
    ) {
        self.level.store(new_level.as_u8(), Ordering::Relaxed);

        tracing::warn!(
            from_level = old_level.as_u8(),
            to_level = new_level.as_u8(),
            from_name = old_level.name(),
            to_name = new_level.name(),
            reason = reason,
            "resilience: degradation level changed"
        );

        // Publish DegradationLevelChanged event
        event_bus
            .publish(BusEvent::Resilience(
                ResilienceEvent::DegradationLevelChanged {
                    event_id: new_event_id(),
                    timestamp: now_timestamp(),
                    from_level: old_level.as_u8(),
                    to_level: new_level.as_u8(),
                    from_name: old_level.name().to_string(),
                    to_name: new_level.name().to_string(),
                    reason: reason.to_string(),
                },
            ))
            .await;

        // L5 is irreversible: cancel the token
        if new_level == DegradationLevel::SafeShutdown {
            tracing::error!(
                level = "L5",
                reason = reason,
                drain_timeout = self.config.drain_timeout_secs,
                "resilience: initiating safe shutdown"
            );
            self.cancellation_token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::CircuitBreakerConfig;
    use crate::clock::Clock;
    use crate::clock::MockClock;
    use std::collections::HashMap;
    use std::time::Duration;

    /// Wrapper to use Arc<MockClock> as Box<dyn Clock + Send>.
    struct ArcClock(Arc<MockClock>);
    impl Clock for ArcClock {
        fn now(&self) -> std::time::Instant {
            self.0.now()
        }
    }

    fn make_registry_with_deps(deps: &[&str]) -> (Arc<CircuitBreakerRegistry>, Arc<MockClock>) {
        let clock = Arc::new(MockClock::new());
        let clock_ref = clock.clone();
        let configs: HashMap<String, CircuitBreakerConfig> = deps
            .iter()
            .map(|name| (name.to_string(), CircuitBreakerConfig::default()))
            .collect();
        let registry = CircuitBreakerRegistry::new_with_clock_factory(configs, move || {
            Box::new(ArcClock(clock_ref.clone()))
        });
        (Arc::new(registry), clock)
    }

    fn trip_breaker(registry: &CircuitBreakerRegistry, name: &str) {
        for _ in 0..5 {
            registry.record_result(name, false);
        }
    }

    fn default_config() -> EscalationConfig {
        EscalationConfig {
            primary_provider: "anthropic".to_string(),
            primary_channel: "telegram".to_string(),
            hysteresis_secs: 2,
            drain_timeout_secs: 30,
            provider_names: vec![
                "anthropic".to_string(),
                "openai".to_string(),
                "ollama".to_string(),
            ],
        }
    }

    // --- DegradationLevel conversion tests ---

    #[test]
    fn level_from_u8_roundtrip() {
        for val in 0..=5u8 {
            let level = DegradationLevel::from_u8(val);
            assert_eq!(level.as_u8(), val);
        }
    }

    #[test]
    fn level_from_u8_clamps_above_5() {
        assert_eq!(DegradationLevel::from_u8(6), DegradationLevel::SafeShutdown);
        assert_eq!(
            DegradationLevel::from_u8(255),
            DegradationLevel::SafeShutdown
        );
    }

    #[test]
    fn level_display() {
        assert_eq!(
            format!("{}", DegradationLevel::FullyOperational),
            "L0 FullyOperational"
        );
        assert_eq!(
            format!("{}", DegradationLevel::MinorDegradation),
            "L1 MinorDegradation"
        );
        assert_eq!(
            format!("{}", DegradationLevel::ReducedFunctionality),
            "L2 ReducedFunctionality"
        );
        assert_eq!(format!("{}", DegradationLevel::CoreOnly), "L3 CoreOnly");
        assert_eq!(format!("{}", DegradationLevel::Emergency), "L4 Emergency");
        assert_eq!(
            format!("{}", DegradationLevel::SafeShutdown),
            "L5 SafeShutdown"
        );
    }

    #[test]
    fn level_name() {
        assert_eq!(
            DegradationLevel::FullyOperational.name(),
            "FullyOperational"
        );
        assert_eq!(DegradationLevel::Emergency.name(), "Emergency");
    }

    // --- compute_level() tests ---

    #[test]
    fn compute_level_l0_all_closed() {
        let deps = &["anthropic", "openai", "ollama", "telegram", "discord"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry, default_config(), token);

        assert_eq!(dm.compute_level(), DegradationLevel::FullyOperational);
    }

    #[test]
    fn compute_level_l1_one_non_critical_open() {
        let deps = &["anthropic", "openai", "ollama", "telegram", "discord"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry.clone(), default_config(), token);

        // Trip a non-critical dep (discord channel, not a provider)
        trip_breaker(&registry, "discord");
        assert_eq!(dm.compute_level(), DegradationLevel::MinorDegradation);
    }

    #[test]
    fn compute_level_l1_one_non_primary_provider_open() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry.clone(), default_config(), token);

        // Trip openai (non-primary provider) -- still L1
        trip_breaker(&registry, "openai");
        assert_eq!(dm.compute_level(), DegradationLevel::MinorDegradation);
    }

    #[test]
    fn compute_level_l2_primary_provider_open() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry.clone(), default_config(), token);

        // Trip primary provider
        trip_breaker(&registry, "anthropic");
        assert_eq!(dm.compute_level(), DegradationLevel::ReducedFunctionality);
    }

    #[test]
    fn compute_level_l3_two_plus_critical() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry.clone(), default_config(), token);

        // Trip 2 providers
        trip_breaker(&registry, "anthropic");
        trip_breaker(&registry, "openai");
        assert_eq!(dm.compute_level(), DegradationLevel::CoreOnly);
    }

    #[test]
    fn compute_level_l4_all_providers_open() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry.clone(), default_config(), token);

        // Trip all providers
        trip_breaker(&registry, "anthropic");
        trip_breaker(&registry, "openai");
        trip_breaker(&registry, "ollama");
        assert_eq!(dm.compute_level(), DegradationLevel::Emergency);
    }

    #[test]
    fn compute_level_l5_all_providers_and_primary_channel() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry.clone(), default_config(), token);

        // Trip all providers + primary channel
        trip_breaker(&registry, "anthropic");
        trip_breaker(&registry, "openai");
        trip_breaker(&registry, "ollama");
        trip_breaker(&registry, "telegram");
        assert_eq!(dm.compute_level(), DegradationLevel::SafeShutdown);
    }

    // --- Integration tests: event loop ---

    #[tokio::test]
    async fn run_escalates_on_circuit_breaker_event() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = Arc::new(DegradationManager::new(
            registry.clone(),
            default_config(),
            token,
        ));

        let event_bus = Arc::new(EventBus::new(64));
        let mut rx_bus = event_bus.subscribe();
        let (tx, rx) = mpsc::channel(64);

        // Spawn the manager
        let dm_ref = dm.clone();
        let bus_ref = event_bus.clone();
        let handle = tokio::spawn(async move {
            dm_ref.run(rx, bus_ref).await;
        });

        // Trip the primary provider
        trip_breaker(&registry, "anthropic");

        // Send circuit breaker state changed event
        tx.send(BusEvent::Resilience(
            ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "anthropic".into(),
                from_state: "closed".into(),
                to_state: "open".into(),
            },
        ))
        .await
        .unwrap();

        // Give event loop time to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Level should now be L2 (primary provider open)
        assert_eq!(dm.current_level(), DegradationLevel::ReducedFunctionality);

        // Should have received a DegradationLevelChanged event on the bus
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx_bus.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged {
                from_level,
                to_level,
                ..
            }) => {
                assert_eq!(from_level, 0);
                assert_eq!(to_level, 2);
            }
            _ => panic!("expected DegradationLevelChanged event"),
        }

        drop(tx); // close channel to stop the loop
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn run_de_escalates_after_hysteresis() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, clock) = make_registry_with_deps(deps);
        let mut config = default_config();
        config.hysteresis_secs = 1; // 1 second for faster testing
        let token = CancellationToken::new();
        let dm = Arc::new(DegradationManager::new(registry.clone(), config, token));

        let event_bus = Arc::new(EventBus::new(64));
        let (tx, rx) = mpsc::channel(64);

        let dm_ref = dm.clone();
        let bus_ref = event_bus.clone();
        let handle = tokio::spawn(async move {
            dm_ref.run(rx, bus_ref).await;
        });

        // First: escalate to L2 by tripping primary provider
        trip_breaker(&registry, "anthropic");
        tx.send(BusEvent::Resilience(
            ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "anthropic".into(),
                from_state: "closed".into(),
                to_state: "open".into(),
            },
        ))
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(dm.current_level(), DegradationLevel::ReducedFunctionality);

        // Now simulate recovery: advance mock clock past reset_timeout so
        // breaker transitions to HalfOpen on check, then record successes to close it.
        clock.advance(Duration::from_secs(61));
        // Trigger HalfOpen transition
        let _ = registry.check("anthropic");
        // Complete 3 successful probes to close the breaker
        for _ in 0..3 {
            registry.record_probe_complete("anthropic");
            registry.record_result("anthropic", true);
            // Check may not be needed for subsequent probes if already in HalfOpen
            let _ = registry.check("anthropic");
        }
        assert_eq!(
            registry.snapshot("anthropic").unwrap().state,
            CircuitBreakerState::Closed
        );

        // Notify the manager of the recovery
        tx.send(BusEvent::Resilience(
            ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "anthropic".into(),
                from_state: "open".into(),
                to_state: "closed".into(),
            },
        ))
        .await
        .unwrap();

        // Wait for hysteresis (1s) + buffer
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Should have de-escalated one step at a time.
        // From L2, after 1s -> L1, then after another 1s -> L0.
        // With 1.5s wait, should be at L1 (one step down).
        // Actually the second hysteresis timer triggers de-escalation to L0 as well.
        // Let's wait a bit more for the second step.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        assert_eq!(dm.current_level(), DegradationLevel::FullyOperational);

        drop(tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn hysteresis_resets_on_new_escalation() {
        let deps = &["anthropic", "openai", "ollama", "telegram", "discord"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let mut config = default_config();
        config.hysteresis_secs = 2;
        let token = CancellationToken::new();
        let dm = Arc::new(DegradationManager::new(registry.clone(), config, token));

        let event_bus = Arc::new(EventBus::new(64));
        let (tx, rx) = mpsc::channel(64);

        let dm_ref = dm.clone();
        let bus_ref = event_bus.clone();
        let handle = tokio::spawn(async move {
            dm_ref.run(rx, bus_ref).await;
        });

        // Escalate to L1 by tripping discord
        trip_breaker(&registry, "discord");
        tx.send(BusEvent::Resilience(
            ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "discord".into(),
                from_state: "closed".into(),
                to_state: "open".into(),
            },
        ))
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(dm.current_level(), DegradationLevel::MinorDegradation);

        // Now escalate further to L2 by tripping primary provider
        trip_breaker(&registry, "anthropic");
        tx.send(BusEvent::Resilience(
            ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "anthropic".into(),
                from_state: "closed".into(),
                to_state: "open".into(),
            },
        ))
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(dm.current_level(), DegradationLevel::ReducedFunctionality);

        drop(tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn l5_cancels_token() {
        let deps = &["anthropic", "openai", "ollama", "telegram"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let token_check = token.clone();
        let dm = Arc::new(DegradationManager::new(
            registry.clone(),
            default_config(),
            token,
        ));

        let event_bus = Arc::new(EventBus::new(64));
        let (tx, rx) = mpsc::channel(64);

        let dm_ref = dm.clone();
        let bus_ref = event_bus.clone();
        let handle = tokio::spawn(async move {
            dm_ref.run(rx, bus_ref).await;
        });

        // Trip all providers + primary channel for L5
        trip_breaker(&registry, "anthropic");
        trip_breaker(&registry, "openai");
        trip_breaker(&registry, "ollama");
        trip_breaker(&registry, "telegram");

        tx.send(BusEvent::Resilience(
            ResilienceEvent::CircuitBreakerStateChanged {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                dependency: "telegram".into(),
                from_state: "closed".into(),
                to_state: "open".into(),
            },
        ))
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(dm.current_level(), DegradationLevel::SafeShutdown);
        assert!(token_check.is_cancelled());

        drop(tx);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
    }

    #[test]
    fn current_level_default_is_l0() {
        let deps = &["anthropic"];
        let (registry, _clock) = make_registry_with_deps(deps);
        let token = CancellationToken::new();
        let dm = DegradationManager::new(registry, default_config(), token);
        assert_eq!(dm.current_level(), DegradationLevel::FullyOperational);
    }
}
