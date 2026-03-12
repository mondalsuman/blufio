// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! RAII recursion guard to prevent hook-triggered-hook infinite loops.
//!
//! Uses an [`AtomicU32`] counter shared across all hooks in a dispatch cycle.
//! Each [`RecursionGuard`] increments on creation and decrements on drop,
//! ensuring the counter stays accurate even if execution panics.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// RAII guard that tracks recursion depth using an atomic counter.
///
/// When a hook triggers another event that would itself trigger hooks,
/// the guard prevents infinite loops by rejecting entry beyond `max_depth`.
///
/// # Example
///
/// ```
/// use std::sync::atomic::AtomicU32;
/// use std::sync::Arc;
/// use blufio_hooks::RecursionGuard;
///
/// let counter = Arc::new(AtomicU32::new(0));
/// let guard = RecursionGuard::try_enter(counter.clone(), 3).unwrap();
/// assert_eq!(RecursionGuard::depth(&counter), 1);
/// drop(guard);
/// assert_eq!(RecursionGuard::depth(&counter), 0);
/// ```
pub struct RecursionGuard {
    counter: Arc<AtomicU32>,
}

impl RecursionGuard {
    /// Try to enter a recursion level. Returns `None` if `max_depth` exceeded.
    ///
    /// The counter is incremented atomically. If the previous value is already
    /// at or above `max_depth`, the increment is rolled back and `None` is
    /// returned.
    pub fn try_enter(counter: Arc<AtomicU32>, max_depth: u32) -> Option<Self> {
        let prev = counter.fetch_add(1, Ordering::SeqCst);
        if prev >= max_depth {
            counter.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self { counter })
    }

    /// Current recursion depth (number of active guards).
    pub fn depth(counter: &AtomicU32) -> u32 {
        counter.load(Ordering::SeqCst)
    }
}

impl Drop for RecursionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_enter_succeeds_at_depth_zero() {
        let counter = Arc::new(AtomicU32::new(0));
        let guard = RecursionGuard::try_enter(counter.clone(), 3);
        assert!(guard.is_some());
        assert_eq!(RecursionGuard::depth(&counter), 1);
    }

    #[test]
    fn try_enter_succeeds_up_to_max_depth_minus_one() {
        let counter = Arc::new(AtomicU32::new(0));
        let max_depth = 3;

        let g1 = RecursionGuard::try_enter(counter.clone(), max_depth);
        assert!(g1.is_some());
        assert_eq!(RecursionGuard::depth(&counter), 1);

        let g2 = RecursionGuard::try_enter(counter.clone(), max_depth);
        assert!(g2.is_some());
        assert_eq!(RecursionGuard::depth(&counter), 2);

        let g3 = RecursionGuard::try_enter(counter.clone(), max_depth);
        assert!(g3.is_some());
        assert_eq!(RecursionGuard::depth(&counter), 3);

        // Keep guards alive for the assertions above.
        drop(g3);
        drop(g2);
        drop(g1);
    }

    #[test]
    fn try_enter_returns_none_at_max_depth() {
        let counter = Arc::new(AtomicU32::new(0));
        let max_depth = 2;

        let _g1 = RecursionGuard::try_enter(counter.clone(), max_depth).unwrap();
        let _g2 = RecursionGuard::try_enter(counter.clone(), max_depth).unwrap();

        // At max_depth, next entry should be rejected.
        let result = RecursionGuard::try_enter(counter.clone(), max_depth);
        assert!(result.is_none());

        // Counter should still be at max_depth (not incremented).
        assert_eq!(RecursionGuard::depth(&counter), 2);
    }

    #[test]
    fn drop_decrements_counter() {
        let counter = Arc::new(AtomicU32::new(0));
        let guard = RecursionGuard::try_enter(counter.clone(), 3).unwrap();
        assert_eq!(RecursionGuard::depth(&counter), 1);

        drop(guard);
        assert_eq!(RecursionGuard::depth(&counter), 0);
    }

    #[test]
    fn counter_returns_to_zero_after_all_guards_dropped() {
        let counter = Arc::new(AtomicU32::new(0));
        let max_depth = 5;

        let guards: Vec<_> = (0..max_depth)
            .map(|_| RecursionGuard::try_enter(counter.clone(), max_depth).unwrap())
            .collect();

        assert_eq!(RecursionGuard::depth(&counter), max_depth);

        drop(guards);
        assert_eq!(RecursionGuard::depth(&counter), 0);
    }

    #[test]
    fn max_depth_one_allows_single_entry() {
        let counter = Arc::new(AtomicU32::new(0));
        let _g = RecursionGuard::try_enter(counter.clone(), 1).unwrap();
        assert!(RecursionGuard::try_enter(counter.clone(), 1).is_none());
        assert_eq!(RecursionGuard::depth(&counter), 1);
    }
}
