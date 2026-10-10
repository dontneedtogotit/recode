use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CIRCUIT_BREAKER_THRESHOLD: usize = 3;
const METRICS_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct CachedMetrics {
    plans: Vec<PlanMetrics>,
    fetched_at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct PlanMetrics {
    pub status: PlanRunStatus,
    pub phases: HashMap<String, Option<PhaseStatus>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanRunStatus {
    #[default]
    Failed,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhaseStatus {
    #[default]
    Failed,
    Completed,
}

#[derive(Debug)]
pub struct PlanCircuitBreaker {
    cache: Mutex<HashMap<String, CachedMetrics>>,
}

impl PlanCircuitBreaker {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Check whether a phase should be skipped because it has failed too many
    /// times recently for the given provider.
    pub fn should_skip_phase(
        &self,
        phase: &str,
        _provider: &str,
        project_id: &str,
    ) -> bool {
        let failures = self.count_recent_failures(phase, project_id);
        failures >= CIRCUIT_BREAKER_THRESHOLD
    }

    /// Reset the failure counter for a phase after a successful run.
    pub fn reset_phase(&self, _phase: &str, project_id: &str) {
        let _ = self.cache.lock().map(|mut cache| cache.remove(&cache_key(project_id)));
    }

    /// Load recent plan metrics for a project. This is the integration point
    /// for callers that already persist metrics elsewhere.
    pub fn load_recent_metrics(&self, project_id: &str, plans: Vec<PlanMetrics>) {
        let key = cache_key(project_id);
        let _ = self.cache.lock().map(|mut cache| {
            cache.insert(
                key,
                CachedMetrics {
                    plans,
                    fetched_at: Instant::now(),
                },
            )
        });
    }

    /// Count recent failures for a phase within the cached window.
    pub fn count_recent_failures(&self, phase: &str, project_id: &str) -> usize {
        let plans = self.get_recent_plans(project_id);
        plans
            .iter()
            .filter(|plan| {
                plan.status == PlanRunStatus::Failed
                    && plan.phases.get(phase).copied().unwrap_or_default() == Some(PhaseStatus::Failed)
            })
            .count()
    }

    /// Force clear the cache for a project.
    pub fn clear_project(&self, project_id: &str) {
        let _ = self.cache.lock().map(|mut cache| cache.remove(&cache_key(project_id)));
    }

    /// Force clear all cached metrics.
    pub fn clear_all(&self) {
        let _ = self.cache.lock().map(|mut cache| cache.clear());
    }

    fn get_recent_plans(&self, project_id: &str) -> Vec<PlanMetrics> {
        let key = cache_key(project_id);
        if let Ok(cache) = self.cache.lock() {
            if let Some(cached) = cache.get(&key) {
                if cached.fetched_at.elapsed() < METRICS_CACHE_TTL {
                    return cached.plans.clone();
                }
            }
        }
        Vec::new()
    }
}

fn cache_key(project_id: &str) -> String {
    format!("circuit:{project_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_metrics(status: PlanRunStatus, phases: Vec<&str>) -> PlanMetrics {
        let mut map = HashMap::new();
        for phase in phases {
            map.insert(phase.to_string(), Some(PhaseStatus::Failed));
        }
        PlanMetrics {
            status,
            phases: map,
        }
    }

    #[test]
    fn should_skip_when_failure_threshold_reached() {
        let breaker = PlanCircuitBreaker::new();
        let plans = vec![
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
        ];
        breaker.load_recent_metrics("project-1", plans);
        assert!(breaker.should_skip_phase("architecture", "provider-a", "project-1"));
    }

    #[test]
    fn should_not_skip_when_below_threshold() {
        let breaker = PlanCircuitBreaker::new();
        let plans = vec![
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
        ];
        breaker.load_recent_metrics("project-1", plans);
        assert!(!breaker.should_skip_phase("architecture", "provider-a", "project-1"));
    }

    #[test]
    fn reset_phase_clears_cached_entries() {
        let breaker = PlanCircuitBreaker::new();
        let plans = vec![
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
        ];
        breaker.load_recent_metrics("project-1", plans);
        assert!(breaker.should_skip_phase("architecture", "provider-a", "project-1"));

        breaker.reset_phase("architecture", "project-1");
        assert!(!breaker.should_skip_phase("architecture", "provider-a", "project-1"));
    }

    #[test]
    fn clear_project_removes_cached_metrics() {
        let breaker = PlanCircuitBreaker::new();
        let plans = vec![
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
            plan_metrics(PlanRunStatus::Failed, vec!["architecture"]),
        ];
        breaker.load_recent_metrics("project-1", plans);
        breaker.clear_project("project-1");
        assert!(!breaker.should_skip_phase("architecture", "provider-a", "project-1"));
    }

    #[test]
    fn count_recent_failures_counts_only_matching_phase() {
        let breaker = PlanCircuitBreaker::new();
        let mut phases = HashMap::new();
        phases.insert("architecture".to_string(), Some(PhaseStatus::Failed));
        phases.insert("testing".to_string(), Some(PhaseStatus::Completed));
        let plans = vec![
            PlanMetrics {
                status: PlanRunStatus::Failed,
                phases: phases.clone(),
            },
            PlanMetrics {
                status: PlanRunStatus::Failed,
                phases,
            },
        ];
        breaker.load_recent_metrics("project-1", plans);
        assert_eq!(
            breaker.count_recent_failures("architecture", "project-1"),
            2
        );
        assert_eq!(breaker.count_recent_failures("testing", "project-1"), 0);
    }
}
