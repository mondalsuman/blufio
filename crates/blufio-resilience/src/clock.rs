// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Clock abstraction for deterministic testing of time-dependent logic.
//!
//! Production code uses [`RealClock`]; tests inject [`MockClock`] to control
//! time advancement without `tokio::time::sleep`.

use std::time::{Duration, Instant};

/// Trait abstracting the system clock for circuit breaker timeout logic.
pub trait Clock: Send + Sync {
    /// Returns the current instant.
    fn now(&self) -> Instant;
}

/// Real system clock for production use.
pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Mock clock for deterministic testing.
///
/// Uses `std::sync::Mutex<Instant>` internally so multiple references can
/// advance time cooperatively.
#[cfg(test)]
pub struct MockClock {
    current: std::sync::Mutex<Instant>,
}

#[cfg(test)]
impl MockClock {
    /// Create a new mock clock anchored at [`Instant::now()`].
    pub fn new() -> Self {
        Self {
            current: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Advance the mock clock by the given duration.
    pub fn advance(&self, duration: Duration) {
        let mut t = self.current.lock().unwrap();
        *t += duration;
    }
}

#[cfg(test)]
impl Clock for MockClock {
    fn now(&self) -> Instant {
        *self.current.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_clock_advances() {
        let clock = RealClock;
        let t1 = clock.now();
        // Spin briefly to ensure time advances.
        std::thread::sleep(Duration::from_millis(1));
        let t2 = clock.now();
        assert!(t2 > t1);
    }

    #[test]
    fn mock_clock_advance() {
        let clock = MockClock::new();
        let t1 = clock.now();
        clock.advance(Duration::from_secs(10));
        let t2 = clock.now();
        assert_eq!(t2 - t1, Duration::from_secs(10));
    }

    #[test]
    fn mock_clock_multiple_advances() {
        let clock = MockClock::new();
        let t1 = clock.now();
        clock.advance(Duration::from_secs(5));
        clock.advance(Duration::from_secs(3));
        let t2 = clock.now();
        assert_eq!(t2 - t1, Duration::from_secs(8));
    }
}
