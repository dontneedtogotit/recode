//! Pattern Learning Service (ported from devcode PatternLearningService.ts)
//!
//! Learns from build outcomes to improve future generations. Records
//! verification failures, repair actions, and their effectiveness so the
//! pipeline can adapt over time.
//! Pure module — no stores, no DI, no I/O.


use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildPattern {
    pub id: String,
    pub category: String, // 'compile' | 'test' | 'lint' | 'browser' | 'persistence' | 'ux' | 'accessibility' | 'production'
    pub pattern: String,
    pub occurrence_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub last_seen_at: String,
    pub examples: Vec<String>,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairActionRecord {
    pub id: String,
    pub pattern_id: String,
    pub action: String,
    pub applied_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub last_applied_at: String,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternLearningInput {
    pub outcome: PatternLearningOutcome,
    pub generated_files: Vec<PatternLearningFile>,
    #[serde(default)]
    pub verification_results: Option<PatternLearningGates>,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternLearningOutcome {
    pub success: bool,
    pub degraded: bool,
    pub failed_checks: Vec<String>,
    pub skipped_checks: Vec<String>,
    pub summary: String,
    pub stage: String,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternLearningFile {
    pub path: String,
    pub content: String,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternLearningGates {
    pub compile_status: Option<String>,
    pub compile_failure_count: Option<usize>,
    pub test_status: Option<String>,
    pub test_tests_passed: Option<usize>,
    pub test_tests_total: Option<usize>,
    pub lint_status: Option<String>,
    pub lint_failure_count: Option<usize>,
    pub browser_status: Option<String>,
    pub browser_console_error_count: Option<usize>,
    pub persistence_status: Option<String>,
    pub persistence_persisted_count: Option<usize>,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatternLearningResult {
    pub patterns: Vec<BuildPattern>,
    pub repair_actions: Vec<RepairActionRecord>,
    pub insights: Vec<String>,
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
pub struct PatternLearningService {
    patterns: BTreeMap<String, BuildPattern>,
    repair_actions: BTreeMap<String, RepairActionRecord>,
}

impl PatternLearningService {
#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn new() -> Self {
        Self {
            patterns: BTreeMap::new(),
            repair_actions: BTreeMap::new(),
        }
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn learn(&mut self, input: &PatternLearningInput) -> PatternLearningResult {
        let mut patterns = Vec::new();
        let mut insights = Vec::new();

        for check_id in &input.outcome.failed_checks {
            let category = categorize_check(check_id);
            let pattern_str = extract_pattern(input, check_id);
            let pattern = self.record_pattern(category, &pattern_str, input.outcome.summary.clone());
            patterns.push(pattern);
        }

        for check_id in &input.outcome.skipped_checks {
            let category = categorize_check(check_id);
            let pattern_str = format!("skipped:{check_id}");
            let pattern = self.record_pattern(category, &pattern_str, input.outcome.summary.clone());
            patterns.push(pattern);
        }

        if !input.outcome.failed_checks.is_empty() {
            insights.push(format!(
                "Build failed on: {}. Consider adding targeted tests or fixes for these checks.",
                input.outcome.failed_checks.join(", ")
            ));
        }
        if input.outcome.degraded {
            insights.push(format!(
                "Build completed in degraded state. Skipped checks: {}.",
                input.outcome.skipped_checks.join(", ")
            ));
        }
        if let Some(gates) = &input.verification_results {
            if let Some(count) = gates.browser_console_error_count {
                if count > 0 {
                    insights.push(format!(
                        "Browser console errors detected ({count}). Review runtime errors."
                    ));
                }
            }
        }

        PatternLearningResult {
            patterns,
            repair_actions: Vec::new(),
            insights,
        }
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn get_patterns(&self) -> Vec<BuildPattern> {
        self.patterns.values().cloned().collect()
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn get_repair_actions(&self) -> Vec<RepairActionRecord> {
        self.repair_actions.values().cloned().collect()
    }

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
    pub fn clear(&mut self) {
        self.patterns.clear();
        self.repair_actions.clear();
    }

    fn record_pattern(&mut self, category: &str, pattern: &str, example: String) -> BuildPattern {
        let key = format!("{category}:{pattern}");
        if let Some(existing) = self.patterns.get_mut(&key) {
            existing.occurrence_count += 1;
            existing.failure_count += 1;
            existing.last_seen_at = now_iso();
            if existing.examples.len() < 5 {
                existing.examples.push(example);
            }
            return existing.clone();
        }
        let p = BuildPattern {
            id: key.clone(),
            category: category.into(),
            pattern: pattern.into(),
            occurrence_count: 1,
            success_count: 0,
            failure_count: 1,
            last_seen_at: now_iso(),
            examples: vec![example],
        };
        self.patterns.insert(key, p.clone());
        p
    }
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
fn categorize_check(check_id: &str) -> &'static str {
    match check_id {
        "compile" => "compile",
        "test" => "test",
        "lint" => "lint",
        "browser" => "browser",
        "persistence" => "persistence",
        "accessibility" => "accessibility",
        "production" => "production",
        _ => "ux",
    }
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
fn extract_pattern(input: &PatternLearningInput, check_id: &str) -> String {
    if let Some(gates) = &input.verification_results {
        match check_id {
            "compile" => {
                if let Some(count) = gates.compile_failure_count {
                    return format!("compile:failed:{count}");
                }
            }
            "lint" => {
                if let Some(count) = gates.lint_failure_count {
                    return format!("lint:failed:{count}");
                }
            }
            "browser" => {
                if let Some(count) = gates.browser_console_error_count {
                    return format!("browser:failed:{count}");
                }
            }
            _ => {}
        }
    }
    format!("{check_id}:failed")
}

#[allow(dead_code)]  // ROADMAP parity port, wiring pending
fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("t{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learn_from_failed_checks() {
        let mut service = PatternLearningService::new();
        let input = PatternLearningInput {
            outcome: PatternLearningOutcome {
                success: false,
                degraded: false,
                failed_checks: vec!["compile".into()],
                skipped_checks: vec![],
                summary: "Build failed".into(),
                stage: "verification".into(),
            },
            generated_files: vec![],
            verification_results: Some(PatternLearningGates {
                compile_status: Some("failed".into()),
                compile_failure_count: Some(5),
                ..Default::default()
            }),
        };
        let result = service.learn(&input);
        assert_eq!(result.patterns.len(), 1);
        assert_eq!(result.patterns[0].category, "compile");
        assert!(result.patterns[0].pattern.contains("compile:failed:5"));
        assert!(!result.insights.is_empty());
    }

    #[test]
    fn test_learn_accumulates() {
        let mut service = PatternLearningService::new();
        let input = PatternLearningInput {
            outcome: PatternLearningOutcome {
                success: false,
                degraded: false,
                failed_checks: vec!["compile".into()],
                skipped_checks: vec![],
                summary: "Build failed".into(),
                stage: "verification".into(),
            },
            generated_files: vec![],
            verification_results: None,
        };
        service.learn(&input);
        service.learn(&input);
        let patterns = service.get_patterns();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].occurrence_count, 2);
        assert_eq!(patterns[0].failure_count, 2);
    }

    #[test]
    fn test_insights_for_degraded() {
        let mut service = PatternLearningService::new();
        let input = PatternLearningInput {
            outcome: PatternLearningOutcome {
                success: true,
                degraded: true,
                failed_checks: vec![],
                skipped_checks: vec!["browser".into()],
                summary: "Build degraded".into(),
                stage: "verification".into(),
            },
            generated_files: vec![],
            verification_results: None,
        };
        let result = service.learn(&input);
        assert!(result.insights.iter().any(|i| i.contains("degraded")));
    }
}
