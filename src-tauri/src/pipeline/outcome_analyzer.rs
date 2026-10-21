//! Build Outcome Analyzer (ported from devcode BuildOutcomeAnalyzer.ts)
//!
//! Analyzes completed build outcomes to identify trends, failure modes,
//! root causes, and recommendations.
//! Pure module — no stores, no DI, no I/O.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeAnalysisOutcome {
    pub success: bool,
    pub degraded: bool,
    pub failed_checks: Vec<String>,
    pub skipped_checks: Vec<String>,
    pub summary: String,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutcomeAnalysisGateResult {
    pub status: String,
    pub failure_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutcomeAnalysisGates {
    pub compile: Option<OutcomeAnalysisGateResult>,
    pub test: Option<OutcomeAnalysisGateResult>,
    pub lint: Option<OutcomeAnalysisGateResult>,
    pub browser: Option<OutcomeAnalysisGateResult>,
    pub persistence: Option<OutcomeAnalysisGateResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildOutcomeAnalysisInput {
    pub outcome: OutcomeAnalysisOutcome,
    pub generated_file_count: usize,
    pub persisted_file_count: usize,
    pub duration_ms: u64,
    #[serde(default)]
    pub verification_results: Option<OutcomeAnalysisGates>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildOutcomeMetrics {
    pub success_rate: f32,
    pub degradation_rate: f32,
    pub common_failures: std::collections::BTreeMap<String, usize>,
    pub avg_duration_ms: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildOutcomeAnalysis {
    pub verdict: String, // 'healthy' | 'degraded' | 'failed'
    pub failure_mode: Option<String>,
    pub root_cause: Option<String>,
    pub recommendations: Vec<String>,
    pub metrics: BuildOutcomeMetrics,
}

pub struct BuildOutcomeAnalyzer {
    history: Vec<BuildOutcomeAnalysisInput>,
}

impl BuildOutcomeAnalyzer {
    pub fn new() -> Self {
        Self { history: Vec::new() }
    }

    pub fn analyze(&mut self, input: BuildOutcomeAnalysisInput) -> BuildOutcomeAnalysis {
        self.history.push(input.clone());

        let verdict = self.determine_verdict(&input);
        let failure_mode = self.identify_failure_mode(&input);
        let root_cause = self.identify_root_cause(&input);
        let recommendations = self.generate_recommendations(&input, failure_mode.as_deref());
        let metrics = self.compute_metrics();

        BuildOutcomeAnalysis {
            verdict,
            failure_mode,
            root_cause,
            recommendations,
            metrics,
        }
    }

#[allow(dead_code)]  // test-only
    pub fn get_history(&self) -> Vec<BuildOutcomeAnalysisInput> {
        self.history.clone()
    }

#[allow(dead_code)]  // test-only
    pub fn clear(&mut self) {
        self.history.clear();
    }

    fn determine_verdict(&self, input: &BuildOutcomeAnalysisInput) -> String {
        if !input.outcome.success {
            return "failed".into();
        }
        if input.outcome.degraded || !input.outcome.skipped_checks.is_empty() {
            return "degraded".into();
        }
        "healthy".into()
    }

    fn identify_failure_mode(&self, input: &BuildOutcomeAnalysisInput) -> Option<String> {
        if input.outcome.failed_checks.is_empty() {
            return None;
        }
        let primary = &input.outcome.failed_checks[0];
        if primary == "compile" {
            if let Some(gates) = &input.verification_results {
                if let Some(compile) = &gates.compile {
                    if compile.failure_count.is_some_and(|c| c > 10) {
                        return Some("mass-compile-failure".into());
                    }
                }
            }
            return Some("compile-failure".into());
        }
        if primary == "test" {
            return Some("test-failure".into());
        }
        if primary == "browser" {
            return Some("runtime-failure".into());
        }
        if primary == "persistence" {
            return Some("persistence-failure".into());
        }
        Some("unknown-failure".into())
    }

    fn identify_root_cause(&self, input: &BuildOutcomeAnalysisInput) -> Option<String> {
        if !input.outcome.success {
            if input.outcome.failed_checks.iter().any(|c| c == "compile") {
                return Some("TypeScript or compilation errors in generated code.".into());
            }
            if input.outcome.failed_checks.iter().any(|c| c == "test") {
                return Some("Generated tests are failing.".into());
            }
            if input.outcome.failed_checks.iter().any(|c| c == "browser") {
                return Some("Application fails to start or crashes at runtime.".into());
            }
            if input.outcome.failed_checks.iter().any(|c| c == "persistence") {
                return Some("Generated files were not persisted correctly.".into());
            }
            return Some("Build failed for an unknown reason.".into());
        }
        if input.outcome.degraded {
            return Some("Build succeeded but verification was incomplete or skipped.".into());
        }
        None
    }

    fn generate_recommendations(&self, input: &BuildOutcomeAnalysisInput, failure_mode: Option<&str>) -> Vec<String> {
        let mut recs = Vec::new();
        match failure_mode {
            Some("mass-compile-failure") => {
                recs.push("Review the generation prompt — the model may be producing invalid TypeScript.".into());
                recs.push("Consider reducing batch size or adding stricter type constraints.".into());
            }
            Some("compile-failure") => {
                recs.push("Run the compiler on individual files to isolate errors.".into());
                recs.push("Check for missing imports or type mismatches.".into());
            }
            Some("test-failure") => {
                recs.push("Review generated test expectations against actual implementation.".into());
                recs.push("Consider adding more context to the test generation prompt.".into());
            }
            Some("runtime-failure") => {
                recs.push("Check browser console for runtime errors.".into());
                recs.push("Verify that the entry point and routing are correctly generated.".into());
            }
            Some("persistence-failure") => {
                recs.push("Check file system permissions and disk space.".into());
                recs.push("Verify the persistence layer is correctly configured.".into());
            }
            _ => {}
        }

        if input.outcome.degraded {
            recs.push("Investigate why verification checks were skipped.".into());
        }
        if input.generated_file_count == 0 {
            recs.push("No files were generated — check the generation pipeline logs.".into());
        }
        if input.duration_ms > 20 * 60 * 1000 {
            recs.push("Build took longer than 20 minutes — consider optimizing generation speed.".into());
        }
        recs
    }

    fn compute_metrics(&self) -> BuildOutcomeMetrics {
        let total = self.history.len();
        let successes = self.history.iter().filter(|h| h.outcome.success).count();
        let degraded = self.history.iter().filter(|h| h.outcome.degraded).count();
        let mut common_failures = std::collections::BTreeMap::new();

        for entry in &self.history {
            for check in &entry.outcome.failed_checks {
                *common_failures.entry(check.clone()).or_insert(0) += 1;
            }
        }

        let total_duration: u64 = self.history.iter().map(|h| h.duration_ms).sum();
        let avg_duration_ms = if total > 0 { total_duration as f32 / total as f32 } else { 0.0 };

        BuildOutcomeMetrics {
            success_rate: if total > 0 { successes as f32 / total as f32 } else { 0.0 },
            degradation_rate: if total > 0 { degraded as f32 / total as f32 } else { 0.0 },
            common_failures,
            avg_duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_outcome_analyzer() {
        let mut analyzer = BuildOutcomeAnalyzer::new();
        let input = BuildOutcomeAnalysisInput {
            outcome: OutcomeAnalysisOutcome {
                success: false,
                degraded: false,
                failed_checks: vec!["compile".into()],
                skipped_checks: Vec::new(),
                summary: "Build failed".into(),
                stage: "compile".into(),
            },
            generated_file_count: 5,
            persisted_file_count: 0,
            duration_ms: 1000,
            verification_results: None,
        };

        let analysis = analyzer.analyze(input);
        assert_eq!(analysis.verdict, "failed");
        assert_eq!(analysis.failure_mode.as_deref(), Some("compile-failure"));
        assert_eq!(analysis.metrics.success_rate, 0.0);
    }
}
