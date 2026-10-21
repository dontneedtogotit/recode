//! Execution-time enforcement of `ManifestBudget`'s runtime caps (P1.7).
//!
//! Pure domain module — no stores, no DI, no I/O. `ManifestBudget` caps are
//! validated at manifest build time (file count, tokens, bytes); this module
//! enforces the two that only exist once generation is actually running:
//!
//! - `max_wall_seconds`: a wall-clock deadline for the whole generation call,
//!   checked at the same safe points as user cancellation.
//! - `max_concurrency`: an in-flight gate around each provider request.
//!
//! ROADMAP §2.2 mandates sequential generation first ("bounded DAG concurrency
//! only after file ownership, leases, cancellation and reconciliation exist"),
//! so today the in-flight counter can never exceed one — the guard enforces
//! the invariant now and becomes the acquire/release seam when waves go
//! parallel. Both violations are `DomainError::conflict` (run-state), distinct
//! from user cancellation by message.

use crate::domain::DomainError;
use crate::pipeline::manifest_budget::ManifestBudget;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Runtime execution limits derived from a manifest budget.
///
/// Not `Clone`: the in-flight counter must live exactly as long as the
/// generation run it guards.
#[derive(Debug)]
pub struct ExecutionLimits {
    /// Wall-clock deadline for the whole generation call, if bounded.
    deadline: Option<Instant>,
    /// Maximum simultaneously in-flight provider requests.
    max_concurrency: usize,
    /// Current in-flight provider requests; guarded slots release on drop.
    in_flight: AtomicUsize,
}

impl ExecutionLimits {
    /// Unbounded limits for callers that run outside a manifest budget
    /// (single-file repairs, tests that opt out).
    pub fn unbounded() -> Self {
        Self {
            deadline: None,
            max_concurrency: usize::MAX,
            in_flight: AtomicUsize::new(0),
        }
    }

    /// Limits derived from the run's manifest budget: the wall-clock cap
    /// starts now; concurrency comes straight from the budget.
    pub fn from_budget(budget: &ManifestBudget) -> Self {
        Self {
            deadline: (budget.max_wall_seconds > 0)
                .then(|| Instant::now() + Duration::from_secs(budget.max_wall_seconds)),
            max_concurrency: budget.max_concurrency.max(1),
            in_flight: AtomicUsize::new(0),
        }
    }

    /// Explicit limits for tests and future callers that need a custom
    /// wall-clock window without a full budget.
    pub fn with_wall_seconds(max_wall_seconds: u64, max_concurrency: usize) -> Self {
        Self {
            deadline: (max_wall_seconds > 0)
                .then(|| Instant::now() + Duration::from_secs(max_wall_seconds)),
            max_concurrency: max_concurrency.max(1),
            in_flight: AtomicUsize::new(0),
        }
    }

    /// True when the wall-clock deadline has passed. Cheap; call at the same
    /// safe points as `should_cancel`.
    pub fn is_expired(&self) -> bool {
        self.deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
    }

    /// Deadline error for an expired run. Message is distinct from user
    /// cancellation so the engine can tell a budget stop from a user stop.
    pub fn expiry_error() -> DomainError {
        DomainError::conflict("generation wall-time budget exceeded")
    }

    /// A guard that keeps one provider request in flight. Dropping the guard
    /// releases the slot, so panics and `?`-returns cannot leak capacity.
    pub fn acquire_in_flight(&self) -> InFlightGuard<'_> {
        let acquired = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        debug_assert!(
            acquired <= self.max_concurrency,
            "generation concurrency {acquired} exceeds cap {}",
            self.max_concurrency
        );
        InFlightGuard { limits: self }
    }

#[allow(dead_code)]  // test-only
    fn current_in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Test-only: limits whose wall-clock deadline is already in the past.
    #[cfg(test)]
    pub(crate) fn expired_for_test() -> Self {
        Self {
            deadline: Some(Instant::now() - Duration::from_nanos(1)),
            max_concurrency: 1,
            in_flight: AtomicUsize::new(0),
        }
    }
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        // Matches `ManifestBudget::default()` so a caller that forgets to pass
        // limits still gets the roadmap caps, not unlimited generation.
        Self::from_budget(&ManifestBudget::default())
    }
}

/// Slot guard: in-flight count is decremented on drop.
#[derive(Debug)]
pub struct InFlightGuard<'a> {
    limits: &'a ExecutionLimits,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.limits.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbounded_never_expires() {
        let limits = ExecutionLimits::unbounded();
        assert!(!limits.is_expired());
        assert_eq!(limits.max_concurrency, usize::MAX);
        assert_eq!(limits.current_in_flight(), 0);
    }

    #[test]
    fn from_budget_default_sets_deadline_and_concurrency() {
        let limits = ExecutionLimits::from_budget(&ManifestBudget::default());
        assert!(
            !limits.is_expired(),
            "fresh 600s deadline must not be expired"
        );
        assert_eq!(limits.max_concurrency, 4);
    }

    #[test]
    fn zero_wall_seconds_means_unbounded_deadline() {
        let budget = ManifestBudget {
            max_wall_seconds: 0,
            ..ManifestBudget::default()
        };
        let limits = ExecutionLimits::from_budget(&budget);
        assert!(!limits.is_expired());
    }

    #[test]
    fn expired_deadline_is_detected() {
        let limits = ExecutionLimits::with_wall_seconds(0, 1);
        assert!(
            !limits.is_expired(),
            "0s means unbounded, not immediately expired"
        );
        // A deadline already in the past is expired by the time we check.
        let past = ExecutionLimits {
            deadline: Some(Instant::now() - Duration::from_nanos(1)),
            max_concurrency: 1,
            in_flight: AtomicUsize::new(0),
        };
        assert!(past.is_expired());
        let err = ExecutionLimits::expiry_error().to_string();
        assert!(
            err.contains("wall-time"),
            "message must be distinct from user cancellation: {err}"
        );
        assert!(!err.contains("cancelled"));
    }

    #[test]
    fn in_flight_guard_releases_slot_on_drop() {
        let limits = ExecutionLimits::with_wall_seconds(60, 2);
        assert_eq!(limits.current_in_flight(), 0);
        {
            let _guard = limits.acquire_in_flight();
            assert_eq!(limits.current_in_flight(), 1);
        }
        assert_eq!(limits.current_in_flight(), 0);
    }

    #[test]
    fn sequential_acquisition_never_releases_past_zero() {
        let limits = ExecutionLimits::with_wall_seconds(60, 1);
        for _ in 0..3 {
            let guard = limits.acquire_in_flight();
            assert_eq!(limits.current_in_flight(), 1);
            drop(guard);
            assert_eq!(limits.current_in_flight(), 0);
        }
    }

    #[test]
    fn default_limits_match_manifest_budget_defaults() {
        let limits = ExecutionLimits::default();
        assert_eq!(limits.max_concurrency, 4);
        assert!(!limits.is_expired());
    }
}
