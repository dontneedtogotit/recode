//! Verification Report Service (ported from devcode VerificationReportService.ts)
//!
//! Generates comprehensive verification reports from build outcomes,
//! combining results from all verification gates into a single structured report.
//! Pure module — no stores, no DI, no I/O.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReportOutcome {
    pub success: bool,
    pub degraded: bool,
    pub failed_checks: Vec<String>,
    pub skipped_checks: Vec<String>,
    pub summary: String,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationGateSummary {
    pub compile_status: Option<String>,
    pub compile_errors: Option<usize>,
    pub test_status: Option<String>,
    pub tests_passed: Option<usize>,
    pub tests_total: Option<usize>,
    pub lint_status: Option<String>,
    pub lint_errors: Option<usize>,
    pub browser_status: Option<String>,
    pub browser_errors: Option<usize>,
    pub persistence_status: Option<String>,
    pub persisted_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReportInput {
    pub outcome: VerificationReportOutcome,
    #[serde(default)]
    pub gate_results: Option<VerificationGateSummary>,
    pub generated_file_count: usize,
    pub persisted_file_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationCheckReport {
    pub id: String,
    pub name: String,
    pub status: String,
    pub error_count: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationReportMetrics {
    pub total_checks: usize,
    pub passed_checks: usize,
    pub failed_checks: usize,
    pub skipped_checks: usize,
    pub pass_rate: f32,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationReport {
    pub overall_status: String,
    pub summary: String,
    pub checks: Vec<VerificationCheckReport>,
    pub metrics: VerificationReportMetrics,
    pub recommendations: Vec<String>,
}

pub struct VerificationReportService;

impl VerificationReportService {
    pub fn generate_report(&self, input: &VerificationReportInput) -> VerificationReport {
        let mut checks = Vec::new();
        let mut recommendations = Vec::new();

        if let Some(gates) = &input.gate_results {
            // Compile
            if let Some(status) = &gates.compile_status {
                let err_count = gates.compile_errors.unwrap_or(0);
                checks.push(VerificationCheckReport {
                    id: "compile".into(),
                    name: "TypeScript Compilation".into(),
                    status: if status == "passed" { "passed".into() } else { "failed".into() },
                    error_count: Some(err_count),
                    message: if status == "passed" {
                        "All files compiled successfully".into()
                    } else {
                        format!("Compilation failed with {err_count} errors")
                    },
                });
            }

            // Test
            if let Some(status) = &gates.test_status {
                let passed = gates.tests_passed.unwrap_or(0);
                let total = gates.tests_total.unwrap_or(0);
                let rate = if total > 0 { (passed as f32 / total as f32) * 100.0 } else { 0.0 };
                checks.push(VerificationCheckReport {
                    id: "test".into(),
                    name: "Test Suite".into(),
                    status: if status == "passed" { "passed".into() } else { "failed".into() },
                    error_count: Some(total.saturating_sub(passed)),
                    message: format!("{passed}/{total} tests passed ({rate:.1}%)"),
                });
            }

            // Lint
            if let Some(status) = &gates.lint_status {
                let err_count = gates.lint_errors.unwrap_or(0);
                checks.push(VerificationCheckReport {
                    id: "lint".into(),
                    name: "Lint".into(),
                    status: if status == "passed" { "passed".into() } else { "failed".into() },
                    error_count: Some(err_count),
                    message: if status == "passed" {
                        "No lint errors".into()
                    } else {
                        format!("{err_count} lint errors found")
                    },
                });
            }

            // Browser
            if let Some(status) = &gates.browser_status {
                let err_count = gates.browser_errors.unwrap_or(0);
                checks.push(VerificationCheckReport {
                    id: "browser".into(),
                    name: "Browser Verification".into(),
                    status: if status == "passed" { "passed".into() } else { "failed".into() },
                    error_count: Some(err_count),
                    message: if status == "passed" {
                        "No console errors".into()
                    } else {
                        format!("{err_count} console errors")
                    },
                });
            }

            // Persistence
            if let Some(status) = &gates.persistence_status {
                let count = gates.persisted_count.unwrap_or(input.persisted_file_count);
                checks.push(VerificationCheckReport {
                    id: "persistence".into(),
                    name: "Persistence Verification".into(),
                    status: if status == "passed" { "passed".into() } else { "failed".into() },
                    error_count: None,
                    message: format!("{count} files persisted"),
                });
            }
        }

        let total_checks = checks.len() + input.outcome.skipped_checks.len();
        let passed_checks = checks.iter().filter(|c| c.status == "passed").count();
        let failed_checks = checks.iter().filter(|c| c.status == "failed").count();
        let skipped_checks = input.outcome.skipped_checks.len();
        let pass_rate = if total_checks > 0 {
            passed_checks as f32 / total_checks as f32
        } else {
            0.0
        };

        if input.outcome.failed_checks.iter().any(|c| c == "compile") {
            recommendations.push("Fix TypeScript compilation errors before proceeding.".into());
        }
        if input.outcome.failed_checks.iter().any(|c| c == "test") {
            recommendations.push("Review and fix failing tests.".into());
        }
        if input.outcome.failed_checks.iter().any(|c| c == "browser") {
            recommendations.push("Address browser console errors to ensure runtime stability.".into());
        }
        if input.outcome.degraded {
            recommendations.push("Investigate skipped checks to ensure full verification coverage.".into());
        }
        if input.generated_file_count == 0 {
            recommendations.push("No files were generated — check the generation pipeline.".into());
        }

        let overall_status = if input.outcome.success {
            if input.outcome.degraded {
                "degraded"
            } else {
                "passed"
            }
        } else {
            "failed"
        };

        VerificationReport {
            overall_status: overall_status.into(),
            summary: input.outcome.summary.clone(),
            checks,
            metrics: VerificationReportMetrics {
                total_checks,
                passed_checks,
                failed_checks,
                skipped_checks,
                pass_rate,
                total_duration_ms: input.duration_ms,
            },
            recommendations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_report_generation() {
        let service = VerificationReportService;
        let input = VerificationReportInput {
            outcome: VerificationReportOutcome {
                success: true,
                degraded: false,
                failed_checks: Vec::new(),
                skipped_checks: vec!["browser".into()],
                summary: "Build succeeded".into(),
                stage: "verification".into(),
            },
            gate_results: Some(VerificationGateSummary {
                compile_status: Some("passed".into()),
                compile_errors: Some(0),
                test_status: Some("passed".into()),
                tests_passed: Some(10),
                tests_total: Some(10),
                ..Default::default()
            }),
            generated_file_count: 5,
            persisted_file_count: 5,
            duration_ms: 1250,
        };

        let report = service.generate_report(&input);
        assert_eq!(report.overall_status, "passed");
        assert_eq!(report.metrics.passed_checks, 2);
        assert_eq!(report.metrics.skipped_checks, 1);
        assert_eq!(report.metrics.total_checks, 3);
        assert!((report.metrics.pass_rate - 0.6666).abs() < 0.01);
    }
}
