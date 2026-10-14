use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderCapabilities {
    pub id: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub preferred_task_types: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionStrategy {
    Priority,
    RoundRobin,
    TaskAware,
    LeastLoaded,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        Self::Priority
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSelectionRecord {
    pub provider_id: String,
    pub task_type: Option<String>,
    pub strategy: SelectionStrategy,
    pub succeeded: bool,
    pub latency_ms: Option<u64>,
    pub selected_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderHealthRecord {
    pub provider_id: String,
    pub status: ProviderHealthStatus,
    pub consecutive_failures: u32,
    pub last_failure: Option<String>,
    pub last_success: Option<String>,
    pub cooldown_until: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSelectionConfig {
    pub strategy: SelectionStrategy,
    pub max_in_flight_per_provider: usize,
    pub degraded_failure_threshold: u32,
    pub unhealthy_failure_threshold: u32,
    pub default_cooldown: Duration,
}

impl Default for ProviderSelectionConfig {
    fn default() -> Self {
        Self {
            strategy: SelectionStrategy::default(),
            max_in_flight_per_provider: 4,
            degraded_failure_threshold: 2,
            unhealthy_failure_threshold: 5,
            default_cooldown: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSelectionEngine {
    config: ProviderSelectionConfig,
    capabilities: HashMap<String, ProviderCapabilities>,
    health: HashMap<String, ProviderHealthRecord>,
    selection_history: Vec<ProviderSelectionRecord>,
    round_robin_cursor: usize,
    in_flight: HashMap<String, usize>,
    /// Per-provider overrides of `config.max_in_flight_per_provider`, set from
    /// calibrated concurrency limits. An unset provider falls back to the
    /// configured default cap.
    max_in_flight_overrides: HashMap<String, usize>,
}

impl ProviderSelectionEngine {
    pub fn new(config: ProviderSelectionConfig) -> Self {
        Self {
            config,
            capabilities: HashMap::new(),
            health: HashMap::new(),
            selection_history: Vec::new(),
            round_robin_cursor: 0,
            in_flight: HashMap::new(),
            max_in_flight_overrides: HashMap::new(),
        }
    }

    /// Override a provider's in-flight cap (e.g. from a calibrated concurrency
    /// limit). Passing `None` clears any override, reverting to the default.
    pub fn set_max_in_flight_override(&mut self, provider_id: &str, limit: Option<usize>) {
        match limit {
            Some(limit) if limit > 0 => {
                self.max_in_flight_overrides
                    .insert(provider_id.into(), limit);
            }
            _ => {
                self.max_in_flight_overrides.remove(provider_id);
            }
        }
    }

    /// The effective in-flight cap for a provider: its calibrated override if
    /// set, else the configured default.
    pub fn effective_in_flight_cap(&self, provider_id: &str) -> usize {
        self.max_in_flight_overrides
            .get(provider_id)
            .copied()
            .unwrap_or(self.config.max_in_flight_per_provider)
    }

    pub fn register_provider(&mut self, capabilities: ProviderCapabilities) {
        self.capabilities
            .insert(capabilities.id.clone(), capabilities.clone());
        self.health
            .entry(capabilities.id.clone())
            .or_insert_with(|| ProviderHealthRecord {
                provider_id: capabilities.id,
                status: ProviderHealthStatus::Unknown,
                consecutive_failures: 0,
                last_failure: None,
                last_success: None,
                cooldown_until: None,
            });
    }

    pub fn update_health(&mut self, provider_id: &str, status: ProviderHealthStatus) {
        if let Some(record) = self.health.get_mut(provider_id) {
            record.status = status;
        }
    }

    pub fn record_outcome(
        &mut self,
        provider_id: &str,
        succeeded: bool,
        latency_ms: Option<u64>,
        task_type: Option<String>,
    ) {
        if let Some(record) = self.health.get_mut(provider_id) {
            if succeeded {
                record.consecutive_failures = 0;
                record.last_success = Some(chrono_now());
                if matches!(
                    record.status,
                    ProviderHealthStatus::Unhealthy | ProviderHealthStatus::Degraded
                ) {
                    record.status = ProviderHealthStatus::Degraded;
                }
            } else {
                record.consecutive_failures += 1;
                record.last_failure = Some(chrono_now());
                if record.consecutive_failures >= self.config.unhealthy_failure_threshold {
                    record.status = ProviderHealthStatus::Unhealthy;
                    record.cooldown_until = Some(chrono_now_plus(self.config.default_cooldown));
                } else if record.consecutive_failures >= self.config.degraded_failure_threshold {
                    record.status = ProviderHealthStatus::Degraded;
                }
            }
        }

        self.selection_history.push(ProviderSelectionRecord {
            provider_id: provider_id.into(),
            task_type,
            strategy: self.config.strategy.clone(),
            succeeded,
            latency_ms,
            selected_at: chrono_now(),
        });
    }

    pub fn select_provider(&mut self, task_type: Option<String>) -> Option<String> {
        let eligible = self.eligible_adapters(task_type.as_deref())?;
        let healthy = self.filter_by_health(eligible);

        if healthy.is_empty() {
            return None;
        }

        match self.config.strategy {
            SelectionStrategy::Priority => self.pick_by_strategy(healthy, |provider| {
                self.health
                    .get(provider)
                    .map(|record| record.consecutive_failures as usize)
                    .unwrap_or(usize::MAX)
            }),
            SelectionStrategy::RoundRobin => self.pick_round_robin(healthy),
            SelectionStrategy::TaskAware => self.pick_by_strategy(healthy, |provider| {
                self.capabilities
                    .get(provider)
                    .map(|capabilities| {
                        let matched = task_type
                            .as_ref()
                            .map(|task_type| capabilities.preferred_task_types.contains(task_type))
                            .unwrap_or(false);
                        if matched {
                            0
                        } else {
                            1
                        }
                    })
                    .unwrap_or(usize::MAX)
            }),
            SelectionStrategy::LeastLoaded => {
                let in_flight_snapshot: HashMap<String, usize> = self
                    .in_flight
                    .iter()
                    .map(|(provider_id, count)| (provider_id.clone(), *count))
                    .collect();
                self.pick_by_strategy(healthy, move |provider| {
                    *in_flight_snapshot.get(provider).unwrap_or(&0)
                })
            }
        }
    }

    pub fn begin_request(&mut self, provider_id: &str) -> bool {
        let current = self.in_flight.get(provider_id).copied().unwrap_or(0);
        if current >= self.effective_in_flight_cap(provider_id) {
            return false;
        }
        self.in_flight.insert(provider_id.into(), current + 1);
        true
    }

    pub fn end_request(&mut self, provider_id: &str) {
        if let Some(current) = self.in_flight.get_mut(provider_id) {
            *current = current.saturating_sub(1);
            if *current == 0 {
                self.in_flight.remove(provider_id);
            }
        }
    }

    pub fn health_status(&self, provider_id: &str) -> Option<ProviderHealthStatus> {
        self.health
            .get(provider_id)
            .map(|record| record.status.clone())
    }

    pub fn consecutive_failures(&self, provider_id: &str) -> Option<u32> {
        self.health
            .get(provider_id)
            .map(|record| record.consecutive_failures)
    }

    pub fn registered_providers(&self) -> Vec<String> {
        self.capabilities.keys().cloned().collect()
    }

    pub fn reset(&mut self) {
        self.selection_history.clear();
        self.round_robin_cursor = 0;
        self.in_flight.clear();
        for record in self.health.values_mut() {
            record.status = ProviderHealthStatus::Unknown;
            record.consecutive_failures = 0;
            record.last_failure = None;
            record.last_success = None;
            record.cooldown_until = None;
        }
    }

    fn eligible_adapters(&self, task_type: Option<&str>) -> Option<Vec<String>> {
        let mut eligible: Vec<String> = self.capabilities.keys().cloned().collect();
        if let Some(task_type) = task_type {
            eligible.retain(|provider_id| {
                self.capabilities
                    .get(provider_id)
                    .map(|capabilities| {
                        capabilities
                            .preferred_task_types
                            .contains(&task_type.to_string())
                    })
                    .unwrap_or(false)
            });
        }
        if eligible.is_empty() {
            return None;
        }
        Some(eligible)
    }

    fn filter_by_health(&self, providers: Vec<String>) -> Vec<String> {
        let mut healthy = providers;
        healthy.retain(|provider_id| {
            self.health
                .get(provider_id)
                .map(|record| {
                    !matches!(
                        record.status,
                        ProviderHealthStatus::Unhealthy | ProviderHealthStatus::Degraded
                    )
                })
                .unwrap_or(true)
        });
        healthy
    }

    fn pick_by_strategy<F: Fn(&str) -> usize>(
        &self,
        providers: Vec<String>,
        score: F,
    ) -> Option<String> {
        let mut best = None;
        let mut best_score = usize::MAX;
        for provider_id in providers {
            let candidate_score = score(&provider_id);
            if candidate_score < best_score {
                best_score = candidate_score;
                best = Some(provider_id);
            }
        }
        best
    }

    fn pick_round_robin(&mut self, mut providers: Vec<String>) -> Option<String> {
        if providers.is_empty() {
            return None;
        }
        let index = self.round_robin_cursor % providers.len();
        self.round_robin_cursor = (self.round_robin_cursor + 1) % providers.len();
        Some(providers.remove(index))
    }
}

fn chrono_now() -> String {
    "2026-01-01T00:00:00Z".into()
}

fn chrono_now_plus(_duration: Duration) -> String {
    "2026-01-01T00:00:30Z".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with_providers(strategy: SelectionStrategy) -> ProviderSelectionEngine {
        let mut engine = ProviderSelectionEngine::new(ProviderSelectionConfig {
            strategy,
            ..ProviderSelectionConfig::default()
        });
        engine.register_provider(ProviderCapabilities {
            id: "primary".into(),
            supports_streaming: true,
            supports_tools: false,
            preferred_task_types: vec!["chat".into(), "summary".into()],
        });
        engine.register_provider(ProviderCapabilities {
            id: "fallback".into(),
            supports_streaming: false,
            supports_tools: true,
            preferred_task_types: vec!["tool".into()],
        });
        engine
    }

    #[test]
    fn task_aware_prefers_matching_capabilities() {
        let mut engine = engine_with_providers(SelectionStrategy::TaskAware);
        let provider_id = engine.select_provider(Some("chat".into()));
        assert_eq!(provider_id, Some("primary".into()));
    }

    #[test]
    fn round_robin_cycles_registered_providers() {
        let mut engine = engine_with_providers(SelectionStrategy::RoundRobin);
        let first = engine.select_provider(None);
        let second = engine.select_provider(None);
        assert!(first.is_some());
        assert!(second.is_some());
        assert_ne!(first, second);
    }

    #[test]
    fn degraded_provider_is_not_selected() {
        let mut engine = engine_with_providers(SelectionStrategy::Priority);
        engine.update_health("primary", ProviderHealthStatus::Degraded);
        engine.update_health("fallback", ProviderHealthStatus::Healthy);
        assert_eq!(engine.select_provider(None), Some("fallback".into()));
    }

    #[test]
    fn unhealthy_provider_is_skipped() {
        let mut engine = engine_with_providers(SelectionStrategy::Priority);
        engine.update_health("primary", ProviderHealthStatus::Unhealthy);
        engine.update_health("fallback", ProviderHealthStatus::Healthy);
        assert_eq!(engine.select_provider(None), Some("fallback".into()));
    }

    #[test]
    fn record_outcome_marks_unhealthy_after_threshold() {
        let mut engine = ProviderSelectionEngine::new(ProviderSelectionConfig {
            strategy: SelectionStrategy::Priority,
            degraded_failure_threshold: 2,
            unhealthy_failure_threshold: 3,
            ..ProviderSelectionConfig::default()
        });
        engine.register_provider(ProviderCapabilities {
            id: "primary".into(),
            supports_streaming: true,
            supports_tools: false,
            preferred_task_types: vec![],
        });
        engine.record_outcome("primary", false, None, None);
        engine.record_outcome("primary", false, None, None);
        engine.record_outcome("primary", false, None, None);
        assert_eq!(
            engine.health_status("primary"),
            Some(ProviderHealthStatus::Unhealthy)
        );
    }

    #[test]
    fn record_outcome_resets_failures_after_success() {
        let mut engine = engine_with_providers(SelectionStrategy::Priority);
        engine.record_outcome("primary", false, None, None);
        engine.record_outcome("primary", false, None, None);
        engine.record_outcome("primary", true, None, None);
        assert_eq!(
            engine.health_status("primary"),
            Some(ProviderHealthStatus::Degraded)
        );
    }

    #[test]
    fn begin_request_respects_in_flight_cap() {
        let mut engine = engine_with_providers(SelectionStrategy::Priority);
        for _ in 0..4 {
            assert!(engine.begin_request("primary"));
        }
        assert!(!engine.begin_request("primary"));
        engine.end_request("primary");
        assert!(engine.begin_request("primary"));
    }

    #[test]
    fn reset_clears_history_and_health() {
        let mut engine = engine_with_providers(SelectionStrategy::Priority);
        engine.record_outcome("primary", false, None, None);
        engine.reset();
        assert_eq!(
            engine.health_status("primary"),
            Some(ProviderHealthStatus::Unknown)
        );
    }
}
