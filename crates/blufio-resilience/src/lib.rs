// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![deny(clippy::unwrap_used)]

//! Resilience primitives for the Blufio agent framework.
//!
//! Provides circuit breakers (3-state FSM), a breaker registry, and
//! supporting types for the degradation ladder.

pub mod circuit_breaker;
pub mod clock;
pub mod degradation;
pub mod registry;
pub mod snapshot;

// Re-export key types for convenience.
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
pub use clock::{Clock, RealClock};
pub use degradation::{DegradationLevel, DegradationManager, EscalationConfig};
pub use registry::CircuitBreakerRegistry;
pub use snapshot::{CircuitBreakerSnapshot, CircuitBreakerState, CircuitBreakerTransition};
