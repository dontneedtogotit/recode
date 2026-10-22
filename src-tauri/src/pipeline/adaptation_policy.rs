//! Pipeline Adaptation Policy (ported from devcode PipelineAdaptationPolicy.ts)
//!
//! Constrained policy layer for the build pipeline. Gates adaptive decisions
//! with hard limits so that adaptive behavior is bounded and auditable.
//! Pure module — no stores, no DI, no I/O.


use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AdaptiveActionType {
    InsertSecurityAudit,
    InsertTypeSafety,
    InsertLinting,
    InsertComplexityAnalysis,
    InsertAccessibilityAudit,
    InsertTestCoverage,
    InsertSpecDrift,
    InsertAgenticReview,
    InsertQualityTelemetry,
    InsertRuntimeSmokeTest,
    InsertRuntimeRemediation,
    RestartBoot,
    SkipStage,
    PauseForReview,
}

impl AdaptiveActionType {
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    fn priority(&self) -> u8 {
        match self {
            Self::InsertRuntimeRemediation | Self::RestartBoot
            | Self::SkipStage | Self::PauseForReview => 0,
            Self::InsertTypeSafety => 1,
            Self::InsertSecurityAudit => 2,
            Self::InsertLinting => 3,
            Self::InsertSpecDrift => 4,
            Self::InsertComplexityAnalysis => 5,
            Self::InsertAccessibilityAudit => 6,
            Self::InsertTestCoverage => 7,
            Self::InsertAgenticReview | Self::InsertRuntimeSmokeTest => 8,
            Self::InsertQualityTelemetry => 9,
        }
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    fn is_non_essential(&self) -> bool {
        matches!(self,
            Self::InsertAgenticReview | Self::InsertQualityTelemetry
            | Self::InsertAccessibilityAudit | Self::InsertTestCoverage
            | Self::InsertSpecDrift | Self::InsertComplexityAnalysis
            | Self::InsertRuntimeSmokeTest
        )
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    fn is_insert(&self) -> bool {
        matches!(self,
            Self::InsertSecurityAudit | Self::InsertTypeSafety
            | Self::InsertLinting | Self::InsertComplexityAnalysis
            | Self::InsertAccessibilityAudit | Self::InsertTestCoverage
            | Self::InsertSpecDrift | Self::InsertAgenticReview
            | Self::InsertQualityTelemetry | Self::InsertRuntimeSmokeTest
        )
    }
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyBudget {
    pub max_inserted_stages: usize,
    pub max_consecutive_timeouts: usize,
    pub max_stage_retries: usize,
    pub min_speculative_hit_rate: f32,
    pub allow_pause_for_review: bool,
    pub max_remediation_insertions: usize,
}

impl Default for PolicyBudget {
    fn default() -> Self {
        Self {
            max_inserted_stages: 6,
            max_consecutive_timeouts: 2,
            max_stage_retries: 3,
            min_speculative_hit_rate: 0.0,
            allow_pause_for_review: true,
            max_remediation_insertions: 1,
        }
    }
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutcome {
    pub stage_name: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    pub retry_count: usize,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeculativeStats {
    pub started: usize,
    pub promoted: usize,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Default)]
pub struct PolicyRunState {
    pub total_inserted: usize,
    pub consecutive_timeouts: usize,
    pub stage_retries: BTreeMap<String, usize>,
    pub stage_outcomes: Vec<StageOutcome>,
    pub speculative_stats: Option<SpeculativeStats>,
    pub vetoed_actions: BTreeSet<AdaptiveActionType>,
    pub remediation_insertions: usize,
    pub started_at: u64,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
pub struct PipelineAdaptationPolicy {
    state: PolicyRunState,
    budget: PolicyBudget,
}

impl PipelineAdaptationPolicy {
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn new(budget: PolicyBudget) -> Self {
        Self {
            state: PolicyRunState::default(),
            budget,
        }
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn record_stage_outcome(&mut self, outcome: StageOutcome) {
        self.state.stage_outcomes.push(outcome.clone());
        let prev = self.state.stage_retries.get(&outcome.stage_name).copied().unwrap_or(0);
        self.state.stage_retries.insert(outcome.stage_name, prev + outcome.retry_count);

        if outcome.error_type.as_deref() == Some("TIMEOUT") {
            self.state.consecutive_timeouts += 1;
        } else if outcome.success {
            self.state.consecutive_timeouts = 0;
        }

        if self.state.consecutive_timeouts >= self.budget.max_consecutive_timeouts {
            for action in [
                AdaptiveActionType::InsertAgenticReview,
                AdaptiveActionType::InsertQualityTelemetry,
                AdaptiveActionType::InsertAccessibilityAudit,
                AdaptiveActionType::InsertTestCoverage,
                AdaptiveActionType::InsertSpecDrift,
                AdaptiveActionType::InsertComplexityAnalysis,
                AdaptiveActionType::InsertRuntimeSmokeTest,
            ] {
                self.state.vetoed_actions.insert(action);
            }
        }
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn update_speculative_stats(&mut self, stats: SpeculativeStats) {
        self.state.speculative_stats = Some(stats);
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn record_inserted(&mut self, count: usize) {
        self.state.total_inserted += count;
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn record_remediation_inserted(&mut self) {
        self.state.remediation_insertions += 1;
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn evaluate(&self, action: &AdaptiveActionType) -> PolicyDecision {
        if self.state.vetoed_actions.contains(action) {
            return PolicyDecision {
                allowed: false,
                reason: format!(
                    "Action vetoed by timeout circuit-breaker ({} consecutive timeouts)",
                    self.state.consecutive_timeouts
                ),
                suggestion: Some("Restore after a successful stage".into()),
            };
        }

        if *action == AdaptiveActionType::InsertRuntimeRemediation {
            if self.state.remediation_insertions >= self.budget.max_remediation_insertions {
                return PolicyDecision {
                    allowed: false,
                    reason: format!(
                        "Remediation budget exhausted ({}/{})",
                        self.state.remediation_insertions, self.budget.max_remediation_insertions
                    ),
                    suggestion: Some("Remaining findings are surfaced to the user instead of repaired".into()),
                };
            }
            return PolicyDecision { allowed: true, reason: "Within remediation budget".into(), suggestion: None };
        }

        if action.is_insert() && self.state.total_inserted >= self.budget.max_inserted_stages {
            return PolicyDecision {
                allowed: false,
                reason: format!(
                    "Stage insertion budget exhausted ({}/{})",
                    self.state.total_inserted, self.budget.max_inserted_stages
                ),
                suggestion: Some("Increase maxInsertedStages or reduce concurrent adaptive stages".into()),
            };
        }

        if *action == AdaptiveActionType::PauseForReview && !self.budget.allow_pause_for_review {
            return PolicyDecision {
                allowed: false,
                reason: "pause_for_review disabled by policy budget".into(),
                suggestion: None,
            };
        }

        PolicyDecision { allowed: true, reason: "Within policy budget".into(), suggestion: None }
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn filter_actions(&self, candidates: &[AdaptiveActionType]) -> Vec<AdaptiveActionType> {
        let budget_remaining = self.budget.max_inserted_stages.saturating_sub(self.state.total_inserted);
        let allowed: Vec<AdaptiveActionType> = candidates
            .iter()
            .filter(|a| self.evaluate(a).allowed)
            .copied()
            .collect();

        let remediation: Vec<AdaptiveActionType> = allowed.iter()
            .filter(|a| matches!(a, AdaptiveActionType::InsertRuntimeRemediation))
            .copied()
            .collect();

        let mut analysis: Vec<AdaptiveActionType> = allowed.iter()
            .filter(|a| !matches!(a, AdaptiveActionType::InsertRuntimeRemediation))
            .copied()
            .collect();
        analysis.sort_by_key(|a| a.priority());
        analysis.truncate(budget_remaining);

        remediation.into_iter().chain(analysis).collect()
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn should_pause_for_review(&self, stage_name: &str) -> bool {
        if !self.budget.allow_pause_for_review { return false; }
        self.state.stage_retries.get(stage_name).copied().unwrap_or(0) >= self.budget.max_stage_retries
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn adaptive_inserted_stages_budget(&self) -> usize {
        let base = self.budget.max_inserted_stages;
        let fault_rate = if !self.state.stage_outcomes.is_empty() {
            self.state.stage_outcomes.iter().filter(|o| !o.success).count() as f32
                / self.state.stage_outcomes.len() as f32
        } else {
            0.0
        };
        let spec_hit = self.state.speculative_stats.as_ref()
            .filter(|s| s.started >= 10)
            .map(|s| s.promoted as f32 / s.started as f32);

        let mut budget = base;
        if fault_rate > 0.75 {
            budget = std::cmp::max(1, (base as f32 * 0.25) as usize);
        } else if fault_rate > 0.5 {
            budget = std::cmp::max(1, (base as f32 * 0.5) as usize);
        }
        if let Some(hit) = spec_hit {
            if hit < 0.3 {
                budget = std::cmp::max(1, (budget as f32 * 0.5) as usize);
            }
        }
        std::cmp::min(base, budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_exhaustion() {
        let mut policy = PipelineAdaptationPolicy::new(PolicyBudget::default());
        policy.record_inserted(6);
        let decision = policy.evaluate(&AdaptiveActionType::InsertLinting);
        assert!(!decision.allowed);
        assert!(decision.reason.contains("budget exhausted"));
    }

    #[test]
    fn test_remediation_separate_budget() {
        let mut policy = PipelineAdaptationPolicy::new(PolicyBudget::default());
        policy.record_inserted(6); // general budget exhausted
        let decision = policy.evaluate(&AdaptiveActionType::InsertRuntimeRemediation);
        assert!(decision.allowed); // remediation has its own budget
        policy.record_remediation_inserted();
        let decision2 = policy.evaluate(&AdaptiveActionType::InsertRuntimeRemediation);
        assert!(!decision2.allowed); // now remediation budget exhausted
    }

    #[test]
    fn test_timeout_circuit_breaker() {
        let mut policy = PipelineAdaptationPolicy::new(PolicyBudget::default());
        policy.record_stage_outcome(StageOutcome {
            stage_name: "gen".into(), success: false, duration_ms: 100,
            error_type: Some("TIMEOUT".into()), retry_count: 1,
        });
        policy.record_stage_outcome(StageOutcome {
            stage_name: "gen".into(), success: false, duration_ms: 100,
            error_type: Some("TIMEOUT".into()), retry_count: 1,
        });
        let decision = policy.evaluate(&AdaptiveActionType::InsertAgenticReview);
        assert!(!decision.allowed);
        assert!(decision.reason.contains("vetoed"));
    }

    #[test]
    fn test_filter_actions_priority() {
        let policy = PipelineAdaptationPolicy::new(PolicyBudget::default());
        let candidates = vec![
            AdaptiveActionType::InsertQualityTelemetry,
            AdaptiveActionType::InsertTypeSafety,
            AdaptiveActionType::InsertLinting,
        ];
        let filtered = policy.filter_actions(&candidates);
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0], AdaptiveActionType::InsertTypeSafety);
        assert_eq!(filtered[1], AdaptiveActionType::InsertLinting);
        assert_eq!(filtered[2], AdaptiveActionType::InsertQualityTelemetry);
    }

    #[test]
    fn test_should_pause_for_review() {
        let mut policy = PipelineAdaptationPolicy::new(PolicyBudget::default());
        policy.record_stage_outcome(StageOutcome {
            stage_name: "gen".into(), success: false, duration_ms: 100,
            error_type: None, retry_count: 3,
        });
        assert!(policy.should_pause_for_review("gen"));
    }

    #[test]
    fn test_adaptive_budget_scales_with_fault_rate() {
        let mut policy = PipelineAdaptationPolicy::new(PolicyBudget::default());
        for _ in 0..8 {
            policy.record_stage_outcome(StageOutcome {
                stage_name: "gen".into(), success: false, duration_ms: 100,
                error_type: None, retry_count: 0,
            });
        }
        let budget = policy.adaptive_inserted_stages_budget();
        assert!(budget < 6); // reduced by high fault rate
    }
}
