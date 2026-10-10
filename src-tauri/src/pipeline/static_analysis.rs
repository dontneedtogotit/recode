//! Static Analysis Service (ported from devcode StaticAnalysisService.ts)
//!
//! Pure static analysis of generated file content: lint-level checks for
//! console statements, TODO/FIXME/XXX comments, and finding aggregation
//! (summary, security score, dedup merge, formatting for review).
//! Pure module — no I/O, no DI; the security-scan tool that devcode imports
//! from `@/modules/agent-platform/public-api` is out of scope for recode.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl Severity {
    pub fn deduction(&self) -> u32 {
        match self {
            Severity::Critical => 25,
            Severity::High => 15,
            Severity::Medium => 8,
            Severity::Low => 3,
            Severity::Info => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisTool {
    Security,
    Lint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticAnalysisFinding {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub category: String,
    pub severity: Severity,
    pub message: String,
    pub suggestion: String,
    pub tool: AnalysisTool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub info: u32,
}

impl AnalysisSummary {
    pub fn from_findings(findings: &[StaticAnalysisFinding]) -> Self {
        let mut s = AnalysisSummary::default();
        for f in findings {
            match f.severity {
                Severity::Critical => s.critical += 1,
                Severity::High => s.high += 1,
                Severity::Medium => s.medium += 1,
                Severity::Low => s.low += 1,
                Severity::Info => s.info += 1,
            }
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticAnalysisResult {
    pub findings: Vec<StaticAnalysisFinding>,
    pub security_score: u32,
    pub files_scanned: usize,
    pub summary: AnalysisSummary,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticAnalysisOptions {
    pub enable_console_check: bool,
    pub enable_todo_check: bool,
    pub severity_threshold: Severity,
}

impl Default for StaticAnalysisOptions {
    fn default() -> Self {
        Self {
            enable_console_check: true,
            enable_todo_check: true,
            severity_threshold: Severity::Low,
        }
    }
}

pub struct StaticAnalysisService;

impl StaticAnalysisService {
    pub fn new() -> Self { Self }

    pub fn detect_language(path: &str) -> &'static str {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "ts" | "tsx" => "ts",
            "js" | "jsx" => "js",
            "py" => "py",
            "go" => "go",
            "rs" => "rs",
            _ => "ts",
        }
    }

    /// Lint a single file: console statements and TODO/FIXME/XXX comments.
    pub fn lint_file(
        path: &str,
        content: &str,
        options: &StaticAnalysisOptions,
    ) -> Vec<StaticAnalysisFinding> {
        let mut findings = Vec::new();

        if options.enable_console_check {
            let mut count = 0u32;
            let lower = content.to_ascii_lowercase();
            for marker in ["console.log", "console.warn", "console.error", "console.info"] {
                let mut start = 0;
                while let Some(pos) = lower[start..].find(marker) {
                    count += 1;
                    start += pos + marker.len();
                }
            }
            if count > 0 {
                findings.push(StaticAnalysisFinding {
                    file: path.to_string(),
                    line: None,
                    category: "Best Practice".into(),
                    severity: Severity::Low,
                    message: format!("Found {} console statement(s)", count),
                    suggestion: "Use Logger service instead of console.log for production code".into(),
                    tool: AnalysisTool::Lint,
                    rule_id: Some("no-console".into()),
                });
            }
        }

        if options.enable_todo_check {
            let mut count = 0u32;
            for line in content.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                    let upper = trimmed.to_ascii_uppercase();
                    if upper.contains(" TODO") || upper.contains(" FIXME") || upper.contains(" XXX") {
                        count += 1;
                    }
                }
            }
            if count > 0 {
                findings.push(StaticAnalysisFinding {
                    file: path.to_string(),
                    line: None,
                    category: "Code Quality".into(),
                    severity: Severity::Info,
                    message: format!("Found {} TODO/FIXME comment(s)", count),
                    suggestion: "Address TODO/FIXME comments or create tracking issues".into(),
                    tool: AnalysisTool::Lint,
                    rule_id: Some("no-todo".into()),
                });
            }
        }

        findings
    }

    /// Analyze a set of files, returning a full result with summary and score.
    pub fn analyze_files(
        files: &[(String, String)], // (path, content)
        options: &StaticAnalysisOptions,
    ) -> StaticAnalysisResult {
        let mut findings = Vec::new();
        let mut scanned = BTreeSet::new();

        for (path, content) in files {
            scanned.insert(path.clone());
            let file_findings = Self::lint_file(path, content, options);
            findings.extend(file_findings);
        }

        let summary = AnalysisSummary::from_findings(&findings);
        let security_score = Self::calculate_security_score(&summary);
        let passed = true;

        StaticAnalysisResult {
            findings,
            security_score,
            files_scanned: scanned.len(),
            summary,
            passed,
        }
    }

    pub fn calculate_security_score(summary: &AnalysisSummary) -> u32 {
        let score = 100u32
            - summary.critical * Severity::Critical.deduction()
            - summary.high * Severity::High.deduction()
            - summary.medium * Severity::Medium.deduction()
            - summary.low * Severity::Low.deduction()
            - summary.info * Severity::Info.deduction();
        score.max(0)
    }

    /// Merge two analysis passes, taking the worst of both. Dedups by
    /// (file, line, severity, message).
    pub fn merge_results(
        earlier: Option<StaticAnalysisResult>,
        later: StaticAnalysisResult,
    ) -> StaticAnalysisResult {
        let Some(earlier) = earlier else {
            return later;
        };
        let mut seen: BTreeSet<(String, Option<u32>, Severity, String)> = BTreeSet::new();
        let mut merged_findings = Vec::new();
        for f in earlier.findings.iter().chain(later.findings.iter()) {
            let key = (f.file.clone(), f.line, f.severity, f.message.clone());
            if seen.insert(key) {
                merged_findings.push(f.clone());
            }
        }
        let summary = AnalysisSummary::from_findings(&merged_findings);
        let security_score = earlier.security_score.min(later.security_score);
        let files_scanned = earlier.files_scanned.max(later.files_scanned);
        let passed = earlier.passed && later.passed;
        StaticAnalysisResult {
            findings: merged_findings,
            security_score,
            files_scanned,
            summary,
            passed,
        }
    }

    pub fn format_findings_for_review(findings: &[StaticAnalysisFinding]) -> Vec<String> {
        findings
            .iter()
            .map(|f| {
                let location = match f.line {
                    Some(l) => format!("{}:{}", f.file, l),
                    None => f.file.clone(),
                };
                let severity = match f.severity {
                    Severity::Critical => "CRITICAL",
                    Severity::High => "HIGH",
                    Severity::Medium => "MEDIUM",
                    Severity::Low => "LOW",
                    Severity::Info => "INFO",
                };
                let tool = match f.tool {
                    AnalysisTool::Security => "SEC",
                    AnalysisTool::Lint => "LINT",
                };
                let rule_id = f.rule_id.as_ref().map(|r| format!(" [{}]", r)).unwrap_or_default();
                format!("[{}] {} {}{}: {}\n  Suggestion: {}", severity, location, tool, rule_id, f.message, f.suggestion)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(StaticAnalysisService::detect_language("App.tsx"), "ts");
        assert_eq!(StaticAnalysisService::detect_language("main.go"), "go");
        assert_eq!(StaticAnalysisService::detect_language("lib.rs"), "rs");
    }

    #[test]
    fn test_lint_console_statements() {
        let content = "console.log('a');\nconsole.error('b');\n";
        let findings = StaticAnalysisService::lint_file("App.tsx", content, &StaticAnalysisOptions::default());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id.as_deref(), Some("no-console"));
        assert_eq!(findings[0].severity, Severity::Low);
    }

    #[test]
    fn test_lint_todo_comments() {
        let content = "// TODO: fix this\n// FIXME: and this\n";
        let findings = StaticAnalysisService::lint_file("App.tsx", content, &StaticAnalysisOptions::default());
        assert!(findings.iter().any(|f| f.rule_id.as_deref() == Some("no-todo")));
    }

    #[test]
    fn test_analyze_files_summary_and_score() {
        let files = vec![
            ("App.tsx".into(), "console.log('x');\n".into()),
            ("utils.ts".into(), "// TODO: fix\n".into()),
        ];
        let result = StaticAnalysisService::analyze_files(&files, &StaticAnalysisOptions::default());
        assert_eq!(result.files_scanned, 2);
        assert_eq!(result.summary.low, 1);
        assert_eq!(result.summary.info, 1);
        assert!(result.security_score < 100);
    }

    #[test]
    fn test_merge_takes_worst_and_dedups() {
        let r1 = StaticAnalysisResult {
            findings: vec![StaticAnalysisFinding {
                file: "A.ts".into(), line: None, category: "x".into(),
                severity: Severity::Low, message: "m".into(), suggestion: "s".into(),
                tool: AnalysisTool::Lint, rule_id: Some("no-console".into()),
            }],
            security_score: 97,
            files_scanned: 1,
            summary: AnalysisSummary { critical: 0, high: 0, medium: 0, low: 1, info: 0 },
            passed: true,
        };
        let r2 = StaticAnalysisResult {
            findings: vec![StaticAnalysisFinding {
                file: "A.ts".into(), line: None, category: "x".into(),
                severity: Severity::Low, message: "m".into(), suggestion: "s".into(),
                tool: AnalysisTool::Lint, rule_id: Some("no-console".into()),
            }],
            security_score: 80,
            files_scanned: 2,
            summary: AnalysisSummary { critical: 0, high: 0, medium: 0, low: 1, info: 0 },
            passed: false,
        };
        let merged = StaticAnalysisService::merge_results(Some(r1), r2);
        assert_eq!(merged.findings.len(), 1); // deduped
        assert_eq!(merged.security_score, 80); // worst
        assert_eq!(merged.files_scanned, 2);
        assert!(!merged.passed);
    }

    #[test]
    fn test_format_findings_for_review() {
        let f = StaticAnalysisFinding {
            file: "App.tsx".into(), line: Some(42), category: "x".into(),
            severity: Severity::High, message: "bad".into(), suggestion: "fix it".into(),
            tool: AnalysisTool::Security, rule_id: Some("SEC001".into()),
        };
        let out = StaticAnalysisService::format_findings_for_review(&[f]);
        assert!(out[0].contains("[HIGH]"));
        assert!(out[0].contains("App.tsx:42"));
        assert!(out[0].contains("[SEC001]"));
    }
}
